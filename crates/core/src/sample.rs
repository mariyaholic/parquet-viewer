// SPDX-License-Identifier: Apache-2.0
//! Virtualized, bounded sample extraction.
//!
//! Samples are read through Arrow with a row selection, so a preview request
//! for page two does not materialize page one. Decompressed bytes, rows,
//! columns and wall-clock time are capped; nested values are rendered with
//! depth and item limits.

use std::time::{Duration, Instant};

use arrow_array::{
    Array, BinaryArray, BooleanArray, Date32Array, Date64Array, Decimal128Array, Decimal256Array,
    Float32Array, Float64Array, Int16Array, Int32Array, Int64Array, Int8Array, LargeBinaryArray,
    LargeListArray, LargeStringArray, ListArray, MapArray, StringArray, StringViewArray,
    StructArray, Time32MillisecondArray, Time32SecondArray, Time64MicrosecondArray,
    Time64NanosecondArray, TimestampMicrosecondArray, TimestampMillisecondArray,
    TimestampNanosecondArray, TimestampSecondArray, UInt16Array, UInt32Array, UInt64Array,
    UInt8Array,
};
use arrow_schema::{DataType, Fields, TimeUnit as ArrowTimeUnit};
use parquet::arrow::arrow_reader::{ParquetRecordBatchReaderBuilder, RowSelection};
use parquet::arrow::ProjectionMask;
use serde::{Deserialize, Serialize};

use crate::display::{escape_text, hex_preview};
use crate::time::{format_date, format_time_of_day, format_timestamp, TimeUnit};
use crate::{Result, Session};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SampleStatus {
    Complete,
    Truncated,
    Unavailable,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SampleRow {
    pub values: Vec<CellValue>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct StructFieldValue {
    pub name: String,
    pub value: CellValue,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "t", content = "v", rename_all = "snake_case")]
pub enum CellValue {
    Null,
    Bool(bool),
    Integer(i64),
    Unsigned(u64),
    Float(String),
    Decimal(String),
    Text(String),
    Binary {
        bytes: usize,
        preview: String,
    },
    Date(String),
    Timestamp(String),
    Time(String),
    Duration(String),
    List {
        items: Vec<CellValue>,
        omitted: usize,
    },
    Map {
        entries: Vec<(CellValue, CellValue)>,
        omitted: usize,
    },
    Struct {
        fields: Vec<StructFieldValue>,
        omitted: usize,
    },
    Unsupported(String),
}

impl CellValue {
    /// Plain-text rendering, already escaped and capped by the converter.
    pub fn render(&self) -> String {
        match self {
            CellValue::Null => "null".to_string(),
            CellValue::Bool(v) => v.to_string(),
            CellValue::Integer(v) => v.to_string(),
            CellValue::Unsigned(v) => v.to_string(),
            CellValue::Float(v) => v.clone(),
            CellValue::Decimal(v) => v.clone(),
            CellValue::Text(v) => v.clone(),
            CellValue::Binary { bytes, preview } => {
                if preview.is_empty() {
                    format!("<{bytes} bytes>")
                } else {
                    format!("<{bytes} bytes: {preview}>")
                }
            }
            CellValue::Date(v) => v.clone(),
            CellValue::Timestamp(v) => v.clone(),
            CellValue::Time(v) => v.clone(),
            CellValue::Duration(v) => v.clone(),
            CellValue::List { items, omitted } => {
                let body = items
                    .iter()
                    .map(CellValue::render)
                    .collect::<Vec<_>>()
                    .join(", ");
                if *omitted > 0 {
                    format!("[{body}, ... {omitted} more]")
                } else {
                    format!("[{body}]")
                }
            }
            CellValue::Map { entries, omitted } => {
                let body = entries
                    .iter()
                    .map(|(k, v)| format!("{}: {}", k.render(), v.render()))
                    .collect::<Vec<_>>()
                    .join(", ");
                if *omitted > 0 {
                    format!("{{{body}, ... {omitted} more}}")
                } else {
                    format!("{{{body}}}")
                }
            }
            CellValue::Struct { fields, omitted } => {
                let body = fields
                    .iter()
                    .map(|f| format!("{}: {}", f.name, f.value.render()))
                    .collect::<Vec<_>>()
                    .join(", ");
                if *omitted > 0 {
                    format!("{{{body}, ... {omitted} more}}")
                } else {
                    format!("{{{body}}}")
                }
            }
            CellValue::Unsupported(reason) => format!("<{reason}>"),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SampleReport {
    pub status: SampleStatus,
    pub offset: u64,
    pub requested: usize,
    pub returned: usize,
    pub total_rows_metadata: u64,
    pub columns: Vec<String>,
    pub column_types: Vec<String>,
    pub omitted_columns: usize,
    pub rows: Vec<SampleRow>,
    pub decoded_bytes: u64,
    pub message: String,
}

impl SampleReport {
    pub fn unavailable(offset: u64, requested: usize, total_rows: u64, message: String) -> Self {
        Self {
            status: SampleStatus::Unavailable,
            offset,
            requested,
            returned: 0,
            total_rows_metadata: total_rows,
            columns: Vec::new(),
            column_types: Vec::new(),
            omitted_columns: 0,
            rows: Vec::new(),
            decoded_bytes: 0,
            message,
        }
    }
}

impl Session {
    /// Read one page of rows. The request is clamped to the compiled limits.
    pub fn sample(&self, offset: u64, limit: usize) -> Result<SampleReport> {
        let limits = self.limits();
        let total_rows = self.summary().row_count.value;
        let offset = offset.min(limits.max_sample_offset);
        let requested = limit.min(limits.max_sample_rows);
        if requested == 0 {
            return Ok(SampleReport::unavailable(
                offset,
                0,
                total_rows,
                "sample limit is zero".to_string(),
            ));
        }
        if total_rows == 0 {
            let mut report = SampleReport::unavailable(
                offset,
                requested,
                total_rows,
                "the file has no rows".to_string(),
            );
            report.status = SampleStatus::Complete;
            return Ok(report);
        }
        if offset >= total_rows {
            let mut report = SampleReport::unavailable(
                offset,
                requested,
                total_rows,
                format!("offset {offset} is beyond the metadata-derived row count"),
            );
            report.status = SampleStatus::Complete;
            return Ok(report);
        }

        let end = offset.saturating_add(requested as u64).min(total_rows);
        let wanted = usize::try_from(end - offset).unwrap_or(requested);
        let selection = RowSelection::from_consecutive_ranges(
            std::iter::once(offset as usize..end as usize),
            usize::try_from(total_rows).unwrap_or(usize::MAX),
        );

        let builder = ParquetRecordBatchReaderBuilder::try_new(self.source().clone())?;
        let arrow_schema = builder.schema().clone();
        let total_columns = arrow_schema.fields().len();
        let shown_columns = total_columns.min(limits.max_sample_columns);
        let projection = ProjectionMask::roots(builder.parquet_schema(), 0..shown_columns);
        let mut needed_bytes = 0u64;
        let mut covered_groups = 0usize;
        let mut start_row = 0u64;
        for group in builder.metadata().row_groups() {
            let rows = u64::try_from(group.num_rows()).unwrap_or(0);
            let group_end = start_row.saturating_add(rows);
            if group_end > offset && start_row < end {
                needed_bytes = needed_bytes
                    .saturating_add(u64::try_from(group.compressed_size().max(0)).unwrap_or(0));
                covered_groups += 1;
            }
            start_row = group_end;
        }
        if needed_bytes > limits.max_sample_compressed_bytes {
            return Ok(SampleReport::unavailable(
                offset,
                requested,
                total_rows,
                format!(
                    "the {covered_groups} row group(s) needed for this page hold {needed_bytes} compressed bytes, above the {}-byte sample budget",
                    limits.max_sample_compressed_bytes
                ),
            ));
        }

        let batch_size = wanted.clamp(1, 256);
        let reader = builder
            .with_projection(projection)
            .with_row_selection(selection)
            .with_batch_size(batch_size)
            .build()?;

        let columns = arrow_schema
            .fields()
            .iter()
            .take(shown_columns)
            .map(|f| escape_text(f.name(), 128))
            .collect::<Vec<_>>();
        let column_types = arrow_schema
            .fields()
            .iter()
            .take(shown_columns)
            .map(|f| data_type_label(f.data_type()))
            .collect::<Vec<_>>();
        let mut rows = Vec::with_capacity(wanted);
        let mut decoded_bytes = 0u64;
        let mut status = SampleStatus::Complete;
        let mut message = format!("requested page of {wanted} row(s) returned");
        let deadline = Instant::now() + Duration::from_millis(limits.max_sample_millis);

        for batch in reader {
            if Instant::now() >= deadline {
                status = SampleStatus::Truncated;
                message = format!(
                    "sample stopped after {} ms; {} of {wanted} requested rows returned",
                    limits.max_sample_millis,
                    rows.len()
                );
                break;
            }
            let batch = batch?;
            let batch_bytes = batch.get_array_memory_size() as u64;
            let remaining = limits
                .max_sample_decoded_bytes
                .saturating_sub(decoded_bytes);
            if batch_bytes > remaining && !rows.is_empty() {
                status = SampleStatus::Truncated;
                message = format!(
                    "decoded-byte budget exhausted; {} of {wanted} requested rows returned",
                    rows.len()
                );
                break;
            }
            decoded_bytes = decoded_bytes.saturating_add(batch_bytes);
            let fields = batch.schema().fields().clone();
            for row in 0..batch.num_rows() {
                if rows.len() >= wanted {
                    break;
                }
                let mut values = Vec::with_capacity(batch.num_columns());
                for (index, field) in fields.iter().enumerate() {
                    let array = batch.column(index);
                    values.push(array_value(
                        array.as_ref(),
                        field.data_type(),
                        row,
                        0,
                        limits.max_nested_depth,
                        limits.max_nested_items,
                        limits.max_cell_chars,
                    ));
                }
                rows.push(SampleRow { values });
            }
            if decoded_bytes >= limits.max_sample_decoded_bytes {
                status = SampleStatus::Truncated;
                message = format!(
                    "decoded-byte budget exhausted; {} of {wanted} requested rows returned",
                    rows.len()
                );
                break;
            }
        }

        if rows.len() < wanted && status == SampleStatus::Complete {
            status = SampleStatus::Truncated;
            message = format!(
                "only {} of {wanted} requested rows could be decoded",
                rows.len()
            );
        }

        Ok(SampleReport {
            status,
            offset,
            requested,
            returned: rows.len(),
            total_rows_metadata: total_rows,
            columns,
            column_types,
            omitted_columns: total_columns.saturating_sub(shown_columns),
            rows,
            decoded_bytes,
            message,
        })
    }
}

fn data_type_label(data_type: &DataType) -> String {
    match data_type {
        DataType::Timestamp(unit, tz) => {
            let unit = match unit {
                ArrowTimeUnit::Second => "s",
                ArrowTimeUnit::Millisecond => "ms",
                ArrowTimeUnit::Microsecond => "us",
                ArrowTimeUnit::Nanosecond => "ns",
            };
            match tz {
                Some(tz) => format!("timestamp({unit}, {})", escape_text(tz, 40)),
                None => format!("timestamp({unit})"),
            }
        }
        other => format!("{other}"),
    }
}

#[allow(clippy::too_many_arguments)]
fn array_value(
    array: &dyn Array,
    data_type: &DataType,
    row: usize,
    depth: usize,
    max_depth: usize,
    max_items: usize,
    max_chars: usize,
) -> CellValue {
    if row >= array.len() {
        return CellValue::Unsupported("row outside array".to_string());
    }
    if array.is_null(row) {
        return CellValue::Null;
    }
    if depth > max_depth {
        return CellValue::Unsupported("nested value beyond display depth".to_string());
    }

    match data_type {
        DataType::Null => CellValue::Null,
        DataType::Boolean => CellValue::Bool(
            array
                .as_any()
                .downcast_ref::<BooleanArray>()
                .map(|a| a.value(row))
                .unwrap_or(false),
        ),
        DataType::Int8 => integer_i8(array, row),
        DataType::Int16 => integer_i16(array, row),
        DataType::Int32 => integer_i32(array, row),
        DataType::Int64 => integer_i64(array, row),
        DataType::UInt8 => unsigned_u8(array, row),
        DataType::UInt16 => unsigned_u16(array, row),
        DataType::UInt32 => unsigned_u32(array, row),
        DataType::UInt64 => unsigned_u64(array, row),
        DataType::Float32 => float32(array, row),
        DataType::Float64 => float64(array, row),
        DataType::Utf8 => text(
            array
                .as_any()
                .downcast_ref::<StringArray>()
                .map(|a| a.value(row)),
            max_chars,
        ),
        DataType::LargeUtf8 => text(
            array
                .as_any()
                .downcast_ref::<LargeStringArray>()
                .map(|a| a.value(row)),
            max_chars,
        ),
        DataType::Utf8View => text(
            array
                .as_any()
                .downcast_ref::<StringViewArray>()
                .map(|a| a.value(row)),
            max_chars,
        ),
        DataType::Binary => binary(
            array
                .as_any()
                .downcast_ref::<BinaryArray>()
                .map(|a| a.value(row)),
            max_chars,
        ),
        DataType::LargeBinary => binary(
            array
                .as_any()
                .downcast_ref::<LargeBinaryArray>()
                .map(|a| a.value(row)),
            max_chars,
        ),
        DataType::FixedSizeBinary(_) => {
            let value = array
                .as_any()
                .downcast_ref::<arrow_array::FixedSizeBinaryArray>()
                .map(|a| a.value(row));
            binary(value, max_chars)
        }
        DataType::Date32 => date32(array, row),
        DataType::Date64 => date64(array, row),
        DataType::Timestamp(unit, tz) => timestamp(array, row, unit, tz.as_deref()),
        DataType::Time32(unit) => time32(array, row, unit),
        DataType::Time64(unit) => time64(array, row, unit),
        DataType::Duration(unit) => duration(array, row, unit),
        DataType::Decimal128(_, scale) => decimal128(array, row, *scale),
        DataType::Decimal256(_, scale) => decimal256(array, row, *scale),
        DataType::List(_) => list_value(array, row, depth, max_depth, max_items, max_chars),
        DataType::LargeList(_) => {
            large_list_value(array, row, depth, max_depth, max_items, max_chars)
        }
        DataType::FixedSizeList(_, _) => {
            fixed_list_value(array, row, depth, max_depth, max_items, max_chars)
        }
        DataType::Struct(fields) => {
            struct_value(array, row, fields, depth, max_depth, max_items, max_chars)
        }
        DataType::Map(_, _) => map_value(array, row, depth, max_depth, max_items, max_chars),
        other => CellValue::Unsupported(format!("unsupported type {other}")),
    }
}

macro_rules! primitive {
    ($name:ident, $ty:ty, $array:ty, $variant:ident) => {
        fn $name(array: &dyn Array, row: usize) -> CellValue {
            array
                .as_any()
                .downcast_ref::<$array>()
                .map(|a| CellValue::$variant(a.value(row).into()))
                .unwrap_or_else(|| CellValue::Unsupported(stringify!($ty).to_string()))
        }
    };
}

primitive!(integer_i8, Int8, Int8Array, Integer);
primitive!(integer_i16, Int16, Int16Array, Integer);
primitive!(integer_i32, Int32, Int32Array, Integer);
primitive!(integer_i64, Int64, Int64Array, Integer);
primitive!(unsigned_u8, UInt8, UInt8Array, Unsigned);
primitive!(unsigned_u16, UInt16, UInt16Array, Unsigned);
primitive!(unsigned_u32, UInt32, UInt32Array, Unsigned);
primitive!(unsigned_u64, UInt64, UInt64Array, Unsigned);

fn float32(array: &dyn Array, row: usize) -> CellValue {
    if let Some(a) = array.as_any().downcast_ref::<Float32Array>() {
        CellValue::Float(format_float(a.value(row) as f64))
    } else {
        CellValue::Unsupported("float32".to_string())
    }
}

fn float64(array: &dyn Array, row: usize) -> CellValue {
    if let Some(a) = array.as_any().downcast_ref::<Float64Array>() {
        CellValue::Float(format_float(a.value(row)))
    } else {
        CellValue::Unsupported("float64".to_string())
    }
}

pub fn format_float(value: f64) -> String {
    if value.is_nan() {
        "NaN".to_string()
    } else if value == f64::INFINITY {
        "inf".to_string()
    } else if value == f64::NEG_INFINITY {
        "-inf".to_string()
    } else {
        value.to_string()
    }
}

fn text(value: Option<&str>, max_chars: usize) -> CellValue {
    match value {
        Some(value) => CellValue::Text(escape_text(value, max_chars)),
        None => CellValue::Null,
    }
}

fn binary(value: Option<&[u8]>, max_chars: usize) -> CellValue {
    match value {
        Some(value) => CellValue::Binary {
            bytes: value.len(),
            preview: hex_preview(value, (max_chars / 2).clamp(8, 64)),
        },
        None => CellValue::Null,
    }
}

fn date32(array: &dyn Array, row: usize) -> CellValue {
    array
        .as_any()
        .downcast_ref::<Date32Array>()
        .map(|a| CellValue::Date(format_date(a.value(row) as i64)))
        .unwrap_or_else(|| CellValue::Unsupported("date32".to_string()))
}

fn date64(array: &dyn Array, row: usize) -> CellValue {
    array
        .as_any()
        .downcast_ref::<Date64Array>()
        .map(|a| CellValue::Date(format_date(a.value(row).div_euclid(86_400_000))))
        .unwrap_or_else(|| CellValue::Unsupported("date64".to_string()))
}

fn timestamp(
    array: &dyn Array,
    row: usize,
    unit: &ArrowTimeUnit,
    timezone: Option<&str>,
) -> CellValue {
    let (value, core_unit) = match unit {
        ArrowTimeUnit::Second => (
            array
                .as_any()
                .downcast_ref::<TimestampSecondArray>()
                .map(|a| a.value(row).saturating_mul(1_000)),
            TimeUnit::Millis,
        ),
        ArrowTimeUnit::Millisecond => (
            array
                .as_any()
                .downcast_ref::<TimestampMillisecondArray>()
                .map(|a| a.value(row)),
            TimeUnit::Millis,
        ),
        ArrowTimeUnit::Microsecond => (
            array
                .as_any()
                .downcast_ref::<TimestampMicrosecondArray>()
                .map(|a| a.value(row)),
            TimeUnit::Micros,
        ),
        ArrowTimeUnit::Nanosecond => (
            array
                .as_any()
                .downcast_ref::<TimestampNanosecondArray>()
                .map(|a| a.value(row)),
            TimeUnit::Nanos,
        ),
    };
    match value {
        Some(value) => {
            let adjusted = timezone.is_some();
            let mut rendered = format_timestamp(value, core_unit, adjusted);
            match timezone {
                Some(tz) => {
                    if !matches!(tz, "UTC" | "utc" | "+00:00" | "Z") {
                        rendered.push_str(&format!(" [{}]", escape_text(tz, 40)));
                    }
                }
                None => rendered.push_str(" (unadjusted)"),
            }
            CellValue::Timestamp(rendered)
        }
        None => CellValue::Null,
    }
}

fn time32(array: &dyn Array, row: usize, unit: &ArrowTimeUnit) -> CellValue {
    let (value, core_unit) = match unit {
        ArrowTimeUnit::Second => (
            array
                .as_any()
                .downcast_ref::<Time32SecondArray>()
                .map(|a| (a.value(row) as i64).saturating_mul(1_000)),
            TimeUnit::Millis,
        ),
        ArrowTimeUnit::Millisecond => (
            array
                .as_any()
                .downcast_ref::<Time32MillisecondArray>()
                .map(|a| a.value(row) as i64),
            TimeUnit::Millis,
        ),
        _ => (None, TimeUnit::Millis),
    };
    match value {
        Some(v) => CellValue::Time(format_time_of_day(v, core_unit)),
        None => CellValue::Unsupported("time32".to_string()),
    }
}

fn time64(array: &dyn Array, row: usize, unit: &ArrowTimeUnit) -> CellValue {
    let (value, core_unit) = match unit {
        ArrowTimeUnit::Microsecond => (
            array
                .as_any()
                .downcast_ref::<Time64MicrosecondArray>()
                .map(|a| a.value(row)),
            TimeUnit::Micros,
        ),
        ArrowTimeUnit::Nanosecond => (
            array
                .as_any()
                .downcast_ref::<Time64NanosecondArray>()
                .map(|a| a.value(row)),
            TimeUnit::Nanos,
        ),
        _ => (None, TimeUnit::Millis),
    };
    match value {
        Some(v) => CellValue::Time(format_time_of_day(v, core_unit)),
        None => CellValue::Unsupported("time64".to_string()),
    }
}

fn duration(array: &dyn Array, row: usize, unit: &ArrowTimeUnit) -> CellValue {
    let value = match unit {
        ArrowTimeUnit::Second => array
            .as_any()
            .downcast_ref::<arrow_array::DurationSecondArray>()
            .map(|a| a.value(row)),
        ArrowTimeUnit::Millisecond => array
            .as_any()
            .downcast_ref::<arrow_array::DurationMillisecondArray>()
            .map(|a| a.value(row)),
        ArrowTimeUnit::Microsecond => array
            .as_any()
            .downcast_ref::<arrow_array::DurationMicrosecondArray>()
            .map(|a| a.value(row)),
        ArrowTimeUnit::Nanosecond => array
            .as_any()
            .downcast_ref::<arrow_array::DurationNanosecondArray>()
            .map(|a| a.value(row)),
    };
    match value {
        Some(v) => {
            let label = match unit {
                ArrowTimeUnit::Second => "s",
                ArrowTimeUnit::Millisecond => "ms",
                ArrowTimeUnit::Microsecond => "us",
                ArrowTimeUnit::Nanosecond => "ns",
            };
            CellValue::Duration(format!("{v} {label}"))
        }
        None => CellValue::Unsupported("duration".to_string()),
    }
}

fn decimal128(array: &dyn Array, row: usize, scale: i8) -> CellValue {
    let raw = array
        .as_any()
        .downcast_ref::<Decimal128Array>()
        .map(|a| a.value(row).to_string());
    match raw {
        Some(raw) => CellValue::Decimal(format_decimal(&raw, scale)),
        None => CellValue::Unsupported("decimal128".to_string()),
    }
}

fn decimal256(array: &dyn Array, row: usize, scale: i8) -> CellValue {
    let raw = array
        .as_any()
        .downcast_ref::<Decimal256Array>()
        .map(|a| a.value(row).to_string());
    match raw {
        Some(raw) => CellValue::Decimal(format_decimal(&raw, scale)),
        None => CellValue::Unsupported("decimal256".to_string()),
    }
}

pub fn format_decimal(digits: &str, scale: i8) -> String {
    let (sign, digits) = match digits.strip_prefix('-') {
        Some(rest) => ("-", rest),
        None => ("", digits),
    };
    if scale <= 0 {
        return format!("{sign}{digits}{}", "0".repeat((-scale) as usize));
    }
    let scale = scale as usize;
    if digits.len() <= scale {
        format!("{sign}0.{}{}", "0".repeat(scale - digits.len()), digits)
    } else {
        let split = digits.len() - scale;
        format!("{sign}{}.{}", &digits[..split], &digits[split..])
    }
}

#[allow(clippy::too_many_arguments)]
fn list_value(
    array: &dyn Array,
    row: usize,
    depth: usize,
    max_depth: usize,
    max_items: usize,
    max_chars: usize,
) -> CellValue {
    let Some(list) = array.as_any().downcast_ref::<ListArray>() else {
        return CellValue::Unsupported("list".to_string());
    };
    nested_items(
        list.values().as_ref(),
        list.value_offsets(),
        row,
        depth,
        max_depth,
        max_items,
        max_chars,
    )
}

#[allow(clippy::too_many_arguments)]
fn large_list_value(
    array: &dyn Array,
    row: usize,
    depth: usize,
    max_depth: usize,
    max_items: usize,
    max_chars: usize,
) -> CellValue {
    let Some(list) = array.as_any().downcast_ref::<LargeListArray>() else {
        return CellValue::Unsupported("large list".to_string());
    };
    let offsets = list.value_offsets();
    let start = usize::try_from(offsets[row]).unwrap_or(0);
    let end = usize::try_from(offsets[row + 1]).unwrap_or(start);
    grid_items(
        list.values().as_ref(),
        start,
        end.saturating_sub(start),
        depth,
        max_depth,
        max_items,
        max_chars,
    )
}

#[allow(clippy::too_many_arguments)]
fn fixed_list_value(
    array: &dyn Array,
    row: usize,
    depth: usize,
    max_depth: usize,
    max_items: usize,
    max_chars: usize,
) -> CellValue {
    let Some(list) = array
        .as_any()
        .downcast_ref::<arrow_array::FixedSizeListArray>()
    else {
        return CellValue::Unsupported("fixed-size list".to_string());
    };
    let values = list.value(row);
    let len = values.len();
    grid_items(
        values.as_ref(),
        0,
        len,
        depth,
        max_depth,
        max_items,
        max_chars,
    )
}

#[allow(clippy::too_many_arguments)]
fn nested_items(
    values: &dyn Array,
    offsets: &[i32],
    row: usize,
    depth: usize,
    max_depth: usize,
    max_items: usize,
    max_chars: usize,
) -> CellValue {
    let start = offsets[row] as usize;
    let len = (offsets[row + 1] - offsets[row]) as usize;
    grid_items(values, start, len, depth, max_depth, max_items, max_chars)
}

#[allow(clippy::too_many_arguments)]
fn grid_items(
    values: &dyn Array,
    start: usize,
    len: usize,
    depth: usize,
    max_depth: usize,
    max_items: usize,
    max_chars: usize,
) -> CellValue {
    let shown = len.min(max_items);
    let mut items = Vec::with_capacity(shown);
    for index in start..start + shown {
        items.push(array_value(
            values,
            values.data_type(),
            index,
            depth + 1,
            max_depth,
            max_items,
            max_chars,
        ));
    }
    CellValue::List {
        items,
        omitted: len.saturating_sub(shown),
    }
}

#[allow(clippy::too_many_arguments)]
fn struct_value(
    array: &dyn Array,
    row: usize,
    fields: &Fields,
    depth: usize,
    max_depth: usize,
    max_items: usize,
    max_chars: usize,
) -> CellValue {
    let Some(structure) = array.as_any().downcast_ref::<StructArray>() else {
        return CellValue::Unsupported("struct".to_string());
    };
    let shown = fields.len().min(max_items);
    let mut values = Vec::with_capacity(shown);
    for index in 0..shown {
        let column = structure.column(index);
        values.push(StructFieldValue {
            name: escape_text(fields[index].name(), 128),
            value: array_value(
                column.as_ref(),
                fields[index].data_type(),
                row,
                depth + 1,
                max_depth,
                max_items,
                max_chars,
            ),
        });
    }
    CellValue::Struct {
        fields: values,
        omitted: fields.len().saturating_sub(shown),
    }
}

#[allow(clippy::too_many_arguments)]
fn map_value(
    array: &dyn Array,
    row: usize,
    depth: usize,
    max_depth: usize,
    max_items: usize,
    max_chars: usize,
) -> CellValue {
    let Some(map) = array.as_any().downcast_ref::<MapArray>() else {
        return CellValue::Unsupported("map".to_string());
    };
    let offsets = map.value_offsets();
    let start = offsets[row] as usize;
    let len = (offsets[row + 1] - offsets[row]) as usize;
    let shown = len.min(max_items);
    let keys = map.keys();
    let values = map.values();
    let mut entries = Vec::with_capacity(shown);
    for index in start..start + shown {
        entries.push((
            array_value(
                keys.as_ref(),
                keys.data_type(),
                index,
                depth + 1,
                max_depth,
                max_items,
                max_chars,
            ),
            array_value(
                values.as_ref(),
                values.data_type(),
                index,
                depth + 1,
                max_depth,
                max_items,
                max_chars,
            ),
        ));
    }
    CellValue::Map {
        entries,
        omitted: len.saturating_sub(shown),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn decimal_formatting() {
        assert_eq!(format_decimal("12345", 2), "123.45");
        assert_eq!(format_decimal("-999", 2), "-9.99");
        assert_eq!(format_decimal("5", 2), "0.05");
        assert_eq!(format_decimal("0", 2), "0.00");
        assert_eq!(format_decimal("12", 0), "12");
        assert_eq!(format_decimal("12", -2), "1200");
    }

    #[test]
    fn float_specials_are_text() {
        assert_eq!(format_float(f64::NAN), "NaN");
        assert_eq!(format_float(f64::INFINITY), "inf");
        assert_eq!(format_float(1.5), "1.5");
    }

    #[test]
    fn cell_rendering_nested() {
        let value = CellValue::List {
            items: vec![
                CellValue::Integer(1),
                CellValue::Struct {
                    fields: vec![StructFieldValue {
                        name: "a".into(),
                        value: CellValue::Text("x".into()),
                    }],
                    omitted: 0,
                },
            ],
            omitted: 2,
        };
        assert_eq!(value.render(), "[1, {a: x}, ... 2 more]");
    }

    #[test]
    fn escaping_applies_to_text() {
        assert_eq!(text(Some("line\nbreak"), 32).render(), "line\\nbreak");
    }
}
