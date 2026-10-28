// SPDX-License-Identifier: Apache-2.0
//! Optional market-data red-flag profile.
//!
//! These are heuristics, not proof of fraud, authenticity or tradability. They
//! run only after the user supplies an explicit field mapping and are always
//! labelled with their sample size and coverage.

use std::collections::{BTreeMap, BTreeSet};
use std::time::{Duration, Instant};

use arrow_array::{Array, RecordBatch};
use arrow_schema::{DataType, TimeUnit as ArrowTimeUnit};
use parquet::arrow::arrow_reader::{ParquetRecordBatchReaderBuilder, RowSelection};
use parquet::arrow::ProjectionMask;
use serde::{Deserialize, Serialize};

use crate::baseline::{ReferenceBaseline, BaselineSpec};
use crate::date_bounds::DateKind;
use crate::display::escape_text;
use crate::sample::format_float;
use crate::time::{format_date, TimeUnit};
use crate::{CancelToken, Result, Session};

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct MarketProfile {
    pub time_field: String,
    pub open_field: String,
    pub high_field: String,
    pub low_field: String,
    pub close_field: String,
    pub spread_field: Option<String>,
    pub instrument: Option<String>,
    #[serde(default)]
    pub thresholds: MarketThresholds,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields, default)]
pub struct MarketThresholds {
    /// Warn when the number of distinct spread values is at most this.
    pub constant_spread_max_distinct: usize,
    /// Warn when open == previous close in more than this fraction.
    pub open_equals_prev_close_max_ratio: f64,
    /// Do not judge the open/previous-close ratio below this many comparisons.
    pub open_equals_prev_close_min_rows: usize,
    /// Warn when absolute excess kurtosis exceeds this.
    pub max_excess_kurtosis: f64,
    /// Warn when this fraction of returns is exactly zero.
    pub max_zero_return_ratio: f64,
    /// Warn when the invalid-price fraction exceeds this.
    pub max_invalid_price_ratio: f64,
    /// Relative tolerance for "equal" prices.
    pub price_tolerance: f64,
    /// Warn when the maximum relative difference from the baseline exceeds this.
    pub baseline_max_relative_diff: f64,
}

impl Default for MarketThresholds {
    fn default() -> Self {
        Self {
            constant_spread_max_distinct: 1,
            open_equals_prev_close_max_ratio: 0.5,
            open_equals_prev_close_min_rows: 10,
            max_excess_kurtosis: 10.0,
            max_zero_return_ratio: 0.2,
            max_invalid_price_ratio: 0.0,
            price_tolerance: 1e-9,
            baseline_max_relative_diff: 0.02,
        }
    }
}

impl MarketProfile {
    pub fn validate(&self) -> crate::Result<()> {
        let fields = [
            ("time_field", &self.time_field),
            ("open_field", &self.open_field),
            ("high_field", &self.high_field),
            ("low_field", &self.low_field),
            ("close_field", &self.close_field),
        ];
        for (name, value) in fields {
            if value.trim().is_empty() {
                return Err(crate::PreviewError::Profile(format!("{name} is empty")));
            }
        }
        if let Some(spread) = &self.spread_field {
            if spread.trim().is_empty() {
                return Err(crate::PreviewError::Profile(
                    "spread_field is empty".to_string(),
                ));
            }
        }
        let t = &self.thresholds;
        let bad_ratio = |value: f64| !value.is_finite() || !(0.0..=1.0).contains(&value);
        if bad_ratio(t.open_equals_prev_close_max_ratio)
            || bad_ratio(t.max_zero_return_ratio)
            || bad_ratio(t.max_invalid_price_ratio)
            || bad_ratio(t.baseline_max_relative_diff)
        {
            return Err(crate::PreviewError::Profile(
                "threshold ratios must be finite and between 0 and 1".to_string(),
            ));
        }
        if !t.max_excess_kurtosis.is_finite() || t.max_excess_kurtosis < 0.0 {
            return Err(crate::PreviewError::Profile(
                "max_excess_kurtosis must be finite and non-negative".to_string(),
            ));
        }
        if !t.price_tolerance.is_finite() || t.price_tolerance < 0.0 {
            return Err(crate::PreviewError::Profile(
                "price_tolerance must be finite and non-negative".to_string(),
            ));
        }
        Ok(())
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum CheckStatus {
    Ok,
    Warning,
    Unavailable,
    NotRun,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct DiagnosticCheck {
    pub id: String,
    pub title: String,
    pub status: CheckStatus,
    pub summary: String,
    pub metrics: BTreeMap<String, String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct TimeRange {
    pub min: String,
    pub max: String,
    pub null_rows: usize,
    pub non_monotonic_steps: usize,
    pub duplicate_steps: usize,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct BaselineComparison {
    pub instrument: String,
    pub source: String,
    pub status: CheckStatus,
    pub matched_days: usize,
    pub data_days: usize,
    pub max_relative_diff: Option<String>,
    pub threshold: String,
    pub summary: String,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct MarketReport {
    pub status: CheckStatus,
    pub instrument: Option<String>,
    pub coverage: String,
    pub rows_used: usize,
    pub total_rows_metadata: u64,
    pub time_range: Option<TimeRange>,
    pub thresholds: MarketThresholds,
    pub checks: Vec<DiagnosticCheck>,
    pub baseline: Option<BaselineComparison>,
    pub notes: Vec<String>,
}

#[derive(Debug, Clone, Default)]
struct Row {
    time: Option<i64>,
    open: Option<f64>,
    high: Option<f64>,
    low: Option<f64>,
    close: Option<f64>,
    spread: Option<f64>,
}

impl Session {
    pub fn diagnose(
        &self,
        profile: &MarketProfile,
        baseline_spec: Option<&BaselineSpec>,
        requested_rows: usize,
        cancel: &CancelToken,
    ) -> Result<MarketReport> {
        profile.validate()?;
        let limits = self.limits();
        let summary = self.summary();
        let total_rows = summary.row_count.value;
        let rows_wanted = requested_rows
            .min(limits.max_diagnostic_rows)
            .min(usize::try_from(total_rows).unwrap_or(usize::MAX));
        let started = Instant::now();
        let deadline = started + Duration::from_millis(limits.max_diagnostic_millis);
        let mut notes = vec![
            "Heuristics are warnings, not proof of fraud, authenticity or integrity.".to_string(),
            "Not financial advice. A baseline only compares the supplied dated prices.".to_string(),
            format!(
                "Field mapping: time={}, open={}, high={}, low={}, close={}, spread={}",
                escape_text(&profile.time_field, 64),
                escape_text(&profile.open_field, 64),
                escape_text(&profile.high_field, 64),
                escape_text(&profile.low_field, 64),
                escape_text(&profile.close_field, 64),
                profile
                    .spread_field
                    .as_deref()
                    .map(|s| escape_text(s, 64))
                    .unwrap_or_else(|| "(derived as high-low)".to_string())
            ),
        ];

        if total_rows == 0 || rows_wanted == 0 {
            return Ok(self.empty_market_report(
                profile,
                "the file has no rows to diagnose".to_string(),
                notes,
            ));
        }
        if cancel.is_cancelled() {
            return Ok(self.empty_market_report(
                profile,
                "diagnostics were cancelled before they started".to_string(),
                notes,
            ));
        }

        let builder = ParquetRecordBatchReaderBuilder::try_new(self.source().clone())?;
        let arrow_schema = builder.schema().clone();
        let mut required = vec![
            profile.time_field.as_str(),
            profile.open_field.as_str(),
            profile.high_field.as_str(),
            profile.low_field.as_str(),
            profile.close_field.as_str(),
        ];
        if let Some(spread) = profile.spread_field.as_deref() {
            required.push(spread);
        }
        let mut unique = BTreeMap::new();
        for name in &required {
            let Some(index) = arrow_schema.fields().iter().position(|f| f.name() == name) else {
                notes.push(format!(
                    "configured field {} is not a top-level column",
                    escape_text(name, 64)
                ));
                return Ok(self.empty_market_report(
                    profile,
                    format!("configured field {} is missing", escape_text(name, 64)),
                    notes,
                ));
            };
            unique.insert(*name, index);
        }
        let time_index = unique[profile.time_field.as_str()];
        let time_data_type = arrow_schema.field(time_index).data_type();
        let Some(time_kind) = arrow_date_kind(time_data_type) else {
            notes.push(format!(
                "time field {} has unsupported type {time_data_type}",
                escape_text(&profile.time_field, 64)
            ));
            return Ok(self.empty_market_report(
                profile,
                "the time field must be a date or timestamp".to_string(),
                notes,
            ));
        };
        for name in [
            profile.open_field.as_str(),
            profile.high_field.as_str(),
            profile.low_field.as_str(),
            profile.close_field.as_str(),
        ] {
            let data_type = arrow_schema.field(unique[name]).data_type();
            if numeric_type(data_type).is_none() {
                notes.push(format!(
                    "field {} has unsupported type {data_type}",
                    escape_text(name, 64)
                ));
                return Ok(self.empty_market_report(
                    profile,
                    format!("field {} is not numeric", escape_text(name, 64)),
                    notes,
                ));
            }
        }
        if let Some(spread) = profile.spread_field.as_deref() {
            let data_type = arrow_schema.field(unique[spread]).data_type();
            if numeric_type(data_type).is_none() {
                return Ok(self.empty_market_report(
                    profile,
                    format!("spread field {} is not numeric", escape_text(spread, 64)),
                    notes,
                ));
            }
        }

        let projection_indices: Vec<usize> = {
            let mut values = unique.values().copied().collect::<Vec<_>>();
            values.sort_unstable();
            values.dedup();
            values
        };
        let projection = ProjectionMask::roots(builder.parquet_schema(), projection_indices);

        let mut needed_bytes = 0u64;
        let mut covered_groups = 0usize;
        let mut start_row = 0u64;
        for group in builder.metadata().row_groups() {
            let rows = u64::try_from(group.num_rows()).unwrap_or(0);
            let group_end = start_row.saturating_add(rows);
            if group_end > 0 && start_row < rows_wanted as u64 {
                needed_bytes = needed_bytes.saturating_add(
                    u64::try_from(group.compressed_size().max(0)).unwrap_or(0),
                );
                covered_groups += 1;
            }
            start_row = group_end;
        }
        if needed_bytes > limits.max_sample_compressed_bytes {
            return Ok(self.empty_market_report(
                profile,
                format!(
                    "the {covered_groups} row group(s) covering the first {rows_wanted} rows hold {needed_bytes} compressed bytes, above the {}-byte diagnostic budget",
                    limits.max_sample_compressed_bytes
                ),
                notes,
            ));
        }

        let selection = if rows_wanted < usize::try_from(total_rows).unwrap_or(usize::MAX) {
            Some(RowSelection::from_consecutive_ranges(
                std::iter::once(0..rows_wanted),
                usize::try_from(total_rows).unwrap_or(usize::MAX),
            ))
        } else {
            None
        };
        let mut reader = builder
            .with_projection(projection)
            .with_batch_size(1024);
        if let Some(selection) = selection {
            reader = reader.with_row_selection(selection);
        }
        let reader = reader.build()?;

        let mut rows = Vec::with_capacity(rows_wanted);
        let mut decoded_bytes = 0u64;
        let mut truncated = false;
        let mut stop_reason: Option<String> = None;
        for batch in reader {
            if cancel.is_cancelled() {
                stop_reason = Some("diagnostics were cancelled".to_string());
                truncated = true;
                break;
            }
            if Instant::now() >= deadline {
                stop_reason = Some(format!(
                    "diagnostics stopped at the {} ms time budget",
                    limits.max_diagnostic_millis
                ));
                truncated = true;
                break;
            }
            let batch = batch?;
            decoded_bytes = decoded_bytes.saturating_add(batch.get_array_memory_size() as u64);
            extract_rows(&batch, profile, time_kind.clone(), &mut rows, rows_wanted);
            if rows.len() >= rows_wanted {
                break;
            }
            if decoded_bytes >= limits.max_sample_decoded_bytes {
                stop_reason = Some(format!(
                    "diagnostics stopped at the {}-byte decoded budget",
                    limits.max_sample_decoded_bytes
                ));
                truncated = true;
                break;
            }
        }
        if let Some(reason) = stop_reason {
            notes.push(reason);
        }
        if rows.is_empty() {
            return Ok(self.empty_market_report(
                profile,
                "no diagnostic rows could be decoded".to_string(),
                notes,
            ));
        }
        if truncated {
            notes.push(format!(
                "coverage is partial: {} of {rows_wanted} requested rows were used",
                rows.len()
            ));
        }

        let time_range = time_range(&rows, &time_kind);
        let mut checks = vec![
            spread_check(&rows, profile, &profile.thresholds),
            open_prev_close_check(&rows, &profile.thresholds),
            return_tail_check(&rows, &profile.thresholds),
            price_plausibility_check(&rows, &profile.thresholds),
            ordering_check(&time_range),
        ];

        let mut baseline = None;
        if let Some(spec) = baseline_spec {
            if !cancel.is_cancelled() {
                match ReferenceBaseline::load(
                    spec,
                    profile.instrument.as_deref(),
                    limits.max_baseline_bytes,
                ) {
                    Ok(reference) => {
                        let comparison = compare_baseline(
                            &rows,
                            &time_kind,
                            &reference,
                            &profile.thresholds,
                        );
                        checks.push(DiagnosticCheck {
                            id: "baseline".to_string(),
                            title: "Reference-price baseline".to_string(),
                            status: comparison.status,
                            summary: comparison.summary.clone(),
                            metrics: baseline_metrics(&comparison),
                        });
                        baseline = Some(comparison);
                    }
                    Err(error) => {
                        checks.push(DiagnosticCheck {
                            id: "baseline".to_string(),
                            title: "Reference-price baseline".to_string(),
                            status: CheckStatus::Unavailable,
                            summary: error.to_string(),
                            metrics: BTreeMap::new(),
                        });
                    }
                }
            }
        } else {
            checks.push(DiagnosticCheck {
                id: "baseline".to_string(),
                title: "Reference-price baseline".to_string(),
                status: CheckStatus::Unavailable,
                summary:
                    "no dated, instrument-specific baseline configured; price comparison unavailable"
                        .to_string(),
                metrics: BTreeMap::new(),
            });
        }

        let status = if checks.iter().any(|c| c.status == CheckStatus::Warning) {
            CheckStatus::Warning
        } else {
            CheckStatus::Ok
        };
        let coverage = format!(
            "first {} of {} rows ({:.1}%); not a full scan",
            rows.len(),
            total_rows,
            (rows.len() as f64 / total_rows.max(1) as f64) * 100.0
        );
        Ok(MarketReport {
            status,
            instrument: profile.instrument.clone(),
            coverage,
            rows_used: rows.len(),
            total_rows_metadata: total_rows,
            time_range: Some(time_range),
            thresholds: profile.thresholds.clone(),
            checks,
            baseline,
            notes,
        })
    }

    fn empty_market_report(
        &self,
        profile: &MarketProfile,
        reason: String,
        mut notes: Vec<String>,
    ) -> MarketReport {
        notes.push(reason.clone());
        MarketReport {
            status: CheckStatus::Unavailable,
            instrument: profile.instrument.clone(),
            coverage: "no rows used".to_string(),
            rows_used: 0,
            total_rows_metadata: self.summary().row_count.value,
            time_range: None,
            thresholds: profile.thresholds.clone(),
            checks: vec![DiagnosticCheck {
                id: "availability".to_string(),
                title: "Diagnostics availability".to_string(),
                status: CheckStatus::Unavailable,
                summary: reason,
                metrics: BTreeMap::new(),
            }],
            baseline: None,
            notes,
        }
    }
}

fn extract_rows(
    batch: &RecordBatch,
    profile: &MarketProfile,
    time_kind: DateKind,
    rows: &mut Vec<Row>,
    limit: usize,
) {
    let schema = batch.schema();
    let find = |name: &str| schema.fields().iter().position(|f| f.name() == name);
    let (Some(time_index), Some(open_index), Some(high_index), Some(low_index), Some(close_index)) = (
        find(&profile.time_field),
        find(&profile.open_field),
        find(&profile.high_field),
        find(&profile.low_field),
        find(&profile.close_field),
    ) else {
        return;
    };
    let spread_index = profile
        .spread_field
        .as_deref()
        .and_then(|name| find(name));
    let time_array = batch.column(time_index);
    let open_array = batch.column(open_index);
    let high_array = batch.column(high_index);
    let low_array = batch.column(low_index);
    let close_array = batch.column(close_index);
    let spread_array = spread_index.map(|index| batch.column(index));
    for row in 0..batch.num_rows() {
        if rows.len() >= limit {
            break;
        }
        rows.push(Row {
            time: raw_time_value(time_array.as_ref(), row, &time_kind),
            open: numeric_value(open_array.as_ref(), row),
            high: numeric_value(high_array.as_ref(), row),
            low: numeric_value(low_array.as_ref(), row),
            close: numeric_value(close_array.as_ref(), row),
            spread: spread_array
                .as_ref()
                .and_then(|array| numeric_value(array.as_ref(), row)),
        });
    }
}

pub fn numeric_type(data_type: &DataType) -> Option<&'static str> {
    match data_type {
        DataType::Float32 | DataType::Float64 => Some("float"),
        DataType::Int8
        | DataType::Int16
        | DataType::Int32
        | DataType::Int64
        | DataType::UInt8
        | DataType::UInt16
        | DataType::UInt32
        | DataType::UInt64 => Some("integer"),
        DataType::Decimal128(_, _) | DataType::Decimal256(_, _) => Some("decimal"),
        _ => None,
    }
}

fn numeric_value(array: &dyn Array, row: usize) -> Option<f64> {
    if row >= array.len() || array.is_null(row) {
        return None;
    }
    match array.data_type() {
        DataType::Float64 => Some(
            array
                .as_any()
                .downcast_ref::<arrow_array::Float64Array>()?
                .value(row),
        ),
        DataType::Float32 => Some(
            array
                .as_any()
                .downcast_ref::<arrow_array::Float32Array>()?
                .value(row) as f64,
        ),
        DataType::Int8 => Some(
            array
                .as_any()
                .downcast_ref::<arrow_array::Int8Array>()?
                .value(row) as f64,
        ),
        DataType::Int16 => Some(
            array
                .as_any()
                .downcast_ref::<arrow_array::Int16Array>()?
                .value(row) as f64,
        ),
        DataType::Int32 => Some(
            array
                .as_any()
                .downcast_ref::<arrow_array::Int32Array>()?
                .value(row) as f64,
        ),
        DataType::Int64 => Some(
            array
                .as_any()
                .downcast_ref::<arrow_array::Int64Array>()?
                .value(row) as f64,
        ),
        DataType::UInt8 => Some(
            array
                .as_any()
                .downcast_ref::<arrow_array::UInt8Array>()?
                .value(row) as f64,
        ),
        DataType::UInt16 => Some(
            array
                .as_any()
                .downcast_ref::<arrow_array::UInt16Array>()?
                .value(row) as f64,
        ),
        DataType::UInt32 => Some(
            array
                .as_any()
                .downcast_ref::<arrow_array::UInt32Array>()?
                .value(row) as f64,
        ),
        DataType::UInt64 => Some(
            array
                .as_any()
                .downcast_ref::<arrow_array::UInt64Array>()?
                .value(row) as f64,
        ),
        DataType::Decimal128(_, scale) => {
            let raw = array
                .as_any()
                .downcast_ref::<arrow_array::Decimal128Array>()?
                .value(row);
            Some(decimal_to_f64(raw as i128, *scale))
        }
        DataType::Decimal256(_, scale) => {
            let raw = array
                .as_any()
                .downcast_ref::<arrow_array::Decimal256Array>()?
                .value(row);
            let value = raw.to_string().parse::<f64>().ok()?;
            Some(value / 10f64.powi(*scale as i32))
        }
        _ => None,
    }
}

fn decimal_to_f64(raw: i128, scale: i8) -> f64 {
    raw as f64 / 10f64.powi(scale as i32)
}

fn arrow_date_kind(data_type: &DataType) -> Option<DateKind> {
    match data_type {
        DataType::Date32 | DataType::Date64 => Some(DateKind::Date),
        DataType::Timestamp(unit, tz) => Some(DateKind::Timestamp {
            unit: match unit {
                ArrowTimeUnit::Second | ArrowTimeUnit::Millisecond => TimeUnit::Millis,
                ArrowTimeUnit::Microsecond => TimeUnit::Micros,
                ArrowTimeUnit::Nanosecond => TimeUnit::Nanos,
            },
            adjusted_to_utc: tz.is_some(),
        }),
        _ => None,
    }
}

fn raw_time_value(array: &dyn Array, row: usize, kind: &DateKind) -> Option<i64> {
    if array.is_null(row) {
        return None;
    }
    match kind {
        DateKind::Date => match array.data_type() {
            DataType::Date32 => Some(
                array
                    .as_any()
                    .downcast_ref::<arrow_array::Date32Array>()?
                    .value(row) as i64,
            ),
            DataType::Date64 => Some(
                array
                    .as_any()
                    .downcast_ref::<arrow_array::Date64Array>()?
                    .value(row)
                    .div_euclid(86_400_000),
            ),
            _ => None,
        },
        DateKind::Timestamp { unit, .. } => match (unit, array.data_type()) {
            (TimeUnit::Millis, DataType::Timestamp(ArrowTimeUnit::Second, _)) => Some(
                array
                    .as_any()
                    .downcast_ref::<arrow_array::TimestampSecondArray>()?
                    .value(row)
                    .saturating_mul(1_000),
            ),
            (TimeUnit::Millis, _) => Some(
                array
                    .as_any()
                    .downcast_ref::<arrow_array::TimestampMillisecondArray>()?
                    .value(row),
            ),
            (TimeUnit::Micros, _) => Some(
                array
                    .as_any()
                    .downcast_ref::<arrow_array::TimestampMicrosecondArray>()?
                    .value(row),
            ),
            (TimeUnit::Nanos, _) => Some(
                array
                    .as_any()
                    .downcast_ref::<arrow_array::TimestampNanosecondArray>()?
                    .value(row),
            ),
        },
    }
}

fn time_range(rows: &[Row], kind: &DateKind) -> TimeRange {
    let mut min: Option<i64> = None;
    let mut max: Option<i64> = None;
    let mut null_rows = 0usize;
    let mut non_monotonic = 0usize;
    let mut duplicates = 0usize;
    let mut previous: Option<i64> = None;
    for row in rows {
        match row.time {
            Some(value) => {
                min = Some(min.map_or(value, |current| current.min(value)));
                max = Some(max.map_or(value, |current| current.max(value)));
                if let Some(previous) = previous {
                    if value < previous {
                        non_monotonic += 1;
                    } else if value == previous {
                        duplicates += 1;
                    }
                }
                previous = Some(value);
            }
            None => null_rows += 1,
        }
    }
    TimeRange {
        min: min.map(|v| kind.format(v)).unwrap_or_else(|| "n/a".to_string()),
        max: max.map(|v| kind.format(v)).unwrap_or_else(|| "n/a".to_string()),
        null_rows,
        non_monotonic_steps: non_monotonic,
        duplicate_steps: duplicates,
    }
}

fn spread_check(rows: &[Row], profile: &MarketProfile, thresholds: &MarketThresholds) -> DiagnosticCheck {
    let mut distinct = BTreeSet::new();
    let mut values = 0usize;
    let mut non_positive = 0usize;
    let mut missing = 0usize;
    let mut constant: Option<f64> = None;
    for row in rows {
        let spread = match profile.spread_field {
            Some(_) => row.spread,
            None => match (row.high, row.low) {
                (Some(high), Some(low)) => Some(high - low),
                _ => None,
            },
        };
        match spread {
            Some(value) if value.is_finite() => {
                values += 1;
                if value <= 0.0 {
                    non_positive += 1;
                }
                distinct.insert(value.to_bits());
                constant = Some(value);
            }
            _ => missing += 1,
        }
    }
    let mut metrics = BTreeMap::new();
    metrics.insert("rows".to_string(), rows.len().to_string());
    metrics.insert("values".to_string(), values.to_string());
    metrics.insert("missing".to_string(), missing.to_string());
    metrics.insert("distinct_values".to_string(), distinct.len().to_string());
    metrics.insert("non_positive".to_string(), non_positive.to_string());
    metrics.insert(
        "threshold_distinct_max".to_string(),
        thresholds.constant_spread_max_distinct.to_string(),
    );
    let (status, summary) = if values == 0 {
        (CheckStatus::Unavailable, "no spread values available".to_string())
    } else if distinct.len() <= thresholds.constant_spread_max_distinct {
        (
            CheckStatus::Warning,
            format!(
                "spread is constant at {} across {} rows; verify the source",
                constant.map(format_float).unwrap_or_default(),
                values
            ),
        )
    } else if non_positive > 0 {
        (
            CheckStatus::Warning,
            format!("{non_positive} of {values} spread values are zero or negative"),
        )
    } else {
        (
            CheckStatus::Ok,
            format!("{} distinct spread values across {values} rows", distinct.len()),
        )
    };
    DiagnosticCheck {
        id: "spread".to_string(),
        title: "Spread constancy".to_string(),
        status,
        summary,
        metrics,
    }
}

fn open_prev_close_check(rows: &[Row], thresholds: &MarketThresholds) -> DiagnosticCheck {
    let mut compared = 0usize;
    let mut equal = 0usize;
    let mut previous: Option<f64> = None;
    for row in rows {
        if let (Some(open), Some(prev)) = (row.open, previous) {
            if open.is_finite() && prev.is_finite() && open > 0.0 && prev > 0.0 {
                compared += 1;
                let tolerance = thresholds.price_tolerance * prev.abs().max(1.0);
                if (open - prev).abs() <= tolerance {
                    equal += 1;
                }
            }
        }
        if let Some(close) = row.close {
            if close.is_finite() {
                previous = Some(close);
            }
        }
    }
    let ratio = if compared == 0 {
        0.0
    } else {
        equal as f64 / compared as f64
    };
    let mut metrics = BTreeMap::new();
    metrics.insert("compared".to_string(), compared.to_string());
    metrics.insert("equal".to_string(), equal.to_string());
    metrics.insert("ratio".to_string(), format!("{ratio:.4}"));
    metrics.insert(
        "threshold_ratio".to_string(),
        format!("{:.4}", thresholds.open_equals_prev_close_max_ratio),
    );
    let (status, summary) = if compared < thresholds.open_equals_prev_close_min_rows {
        (
            CheckStatus::NotRun,
            format!(
                "only {compared} comparisons; at least {} are required",
                thresholds.open_equals_prev_close_min_rows
            ),
        )
    } else if ratio > thresholds.open_equals_prev_close_max_ratio {
        (
            CheckStatus::Warning,
            format!(
                "open equals the previous close in {equal} of {compared} comparisons ({ratio:.1}%)"
            ),
        )
    } else {
        (
            CheckStatus::Ok,
            format!("open equals the previous close in {equal} of {compared} comparisons"),
        )
    };
    DiagnosticCheck {
        id: "open_prev_close".to_string(),
        title: "Open versus previous close".to_string(),
        status,
        summary,
        metrics,
    }
}

fn return_tail_check(rows: &[Row], thresholds: &MarketThresholds) -> DiagnosticCheck {
    let mut returns = Vec::new();
    let mut previous: Option<f64> = None;
    let mut zero = 0usize;
    for row in rows {
        if let Some(close) = row.close {
            if close.is_finite() && close > 0.0 {
                if let Some(prev) = previous {
                    if prev > 0.0 {
                        let value = (close / prev).ln();
                        if value.is_finite() {
                            if value == 0.0 {
                                zero += 1;
                            }
                            returns.push(value);
                        }
                    }
                }
                previous = Some(close);
            }
        }
    }
    let mut metrics = BTreeMap::new();
    metrics.insert("returns".to_string(), returns.len().to_string());
    if returns.len() < 4 {
        return DiagnosticCheck {
            id: "return_tails".to_string(),
            title: "Return-tail distribution".to_string(),
            status: CheckStatus::NotRun,
            summary: format!("only {} returns available", returns.len()),
            metrics,
        };
    }
    let n = returns.len() as f64;
    let mean = returns.iter().sum::<f64>() / n;
    let m2 = returns.iter().map(|r| (r - mean).powi(2)).sum::<f64>() / n;
    let m4 = returns.iter().map(|r| (r - mean).powi(4)).sum::<f64>() / n;
    let zero_ratio = zero as f64 / returns.len() as f64;
    let excess = if m2 > 0.0 { m4 / (m2 * m2) - 3.0 } else { f64::NAN };
    metrics.insert("zero_returns".to_string(), zero.to_string());
    metrics.insert("zero_ratio".to_string(), format!("{zero_ratio:.4}"));
    metrics.insert(
        "excess_kurtosis".to_string(),
        if excess.is_finite() {
            format!("{excess:.3}")
        } else {
            "undefined".to_string()
        },
    );
    metrics.insert(
        "threshold_excess".to_string(),
        format!("{:.2}", thresholds.max_excess_kurtosis),
    );
    metrics.insert(
        "threshold_zero_ratio".to_string(),
        format!("{:.4}", thresholds.max_zero_return_ratio),
    );
    let mut warnings = Vec::new();
    if zero_ratio > thresholds.max_zero_return_ratio {
        warnings.push(format!(
            "{:.1}% of returns are exactly zero",
            zero_ratio * 100.0
        ));
    }
    if excess.is_finite() && excess.abs() > thresholds.max_excess_kurtosis {
        warnings.push(format!("excess kurtosis {excess:.2}"));
    }
    let (status, summary) = if warnings.is_empty() {
        (
            CheckStatus::Ok,
            format!(
                "{} returns; excess kurtosis {}; zero-return ratio {:.1}%",
                returns.len(),
                if excess.is_finite() {
                    format!("{excess:.2}")
                } else {
                    "undefined".to_string()
                },
                zero_ratio * 100.0
            ),
        )
    } else {
        (
            CheckStatus::Warning,
            format!("{} warn(s): {}", warnings.len(), warnings.join("; ")),
        )
    };
    DiagnosticCheck {
        id: "return_tails".to_string(),
        title: "Return-tail distribution".to_string(),
        status,
        summary,
        metrics,
    }
}

fn price_plausibility_check(rows: &[Row], thresholds: &MarketThresholds) -> DiagnosticCheck {
    let mut invalid = 0usize;
    let mut examples = 0usize;
    for row in rows {
        let mut bad = false;
        for value in [row.open, row.high, row.low, row.close].into_iter().flatten() {
            if !value.is_finite() || value <= 0.0 {
                bad = true;
            }
        }
        if let (Some(high), Some(low)) = (row.high, row.low) {
            if high < low {
                bad = true;
            }
            let tolerance = thresholds.price_tolerance * high.abs().max(1.0);
            if let Some(open) = row.open {
                if open < low - tolerance || open > high + tolerance {
                    bad = true;
                }
            }
            if let Some(close) = row.close {
                if close < low - tolerance || close > high + tolerance {
                    bad = true;
                }
            }
        }
        if bad {
            invalid += 1;
            if examples < 3 {
                examples += 1;
            }
        }
    }
    let ratio = invalid as f64 / rows.len().max(1) as f64;
    let mut metrics = BTreeMap::new();
    metrics.insert("rows".to_string(), rows.len().to_string());
    metrics.insert("invalid_rows".to_string(), invalid.to_string());
    metrics.insert("invalid_ratio".to_string(), format!("{ratio:.4}"));
    metrics.insert(
        "threshold_ratio".to_string(),
        format!("{:.4}", thresholds.max_invalid_price_ratio),
    );
    let (status, summary) = if ratio > thresholds.max_invalid_price_ratio {
        (
            CheckStatus::Warning,
            format!("{invalid} of {} rows have implausible OHLC values", rows.len()),
        )
    } else {
        (
            CheckStatus::Ok,
            "OHLC values are finite, positive and internally consistent".to_string(),
        )
    };
    DiagnosticCheck {
        id: "price_plausibility".to_string(),
        title: "Price plausibility".to_string(),
        status,
        summary,
        metrics,
    }
}

fn ordering_check(range: &TimeRange) -> DiagnosticCheck {
    let mut metrics = BTreeMap::new();
    metrics.insert("null_rows".to_string(), range.null_rows.to_string());
    metrics.insert(
        "non_monotonic_steps".to_string(),
        range.non_monotonic_steps.to_string(),
    );
    metrics.insert("duplicate_steps".to_string(), range.duplicate_steps.to_string());
    let status = if range.non_monotonic_steps > 0 {
        CheckStatus::Warning
    } else {
        CheckStatus::Ok
    };
    let summary = if range.null_rows > 0 || range.non_monotonic_steps > 0 {
        format!(
            "{} null time value(s), {} non-monotonic step(s), {} duplicate step(s); file order was used",
            range.null_rows, range.non_monotonic_steps, range.duplicate_steps
        )
    } else {
        "time is non-decreasing in file order".to_string()
    };
    DiagnosticCheck {
        id: "time_order".to_string(),
        title: "Time ordering".to_string(),
        status,
        summary,
        metrics,
    }
}

fn compare_baseline(
    rows: &[Row],
    kind: &DateKind,
    reference: &ReferenceBaseline,
    thresholds: &MarketThresholds,
) -> BaselineComparison {
    let mut last_close: BTreeMap<i64, f64> = BTreeMap::new();
    for row in rows {
        if let (Some(time), Some(close)) = (row.time, row.close) {
            if close.is_finite() && close > 0.0 {
                last_close.insert(day_of(kind, time), close);
            }
        }
    }
    let mut matched = 0usize;
    let mut max_diff: Option<f64> = None;
    for (day, close) in &last_close {
        if let Some(reference_price) = reference.points.get(day) {
            matched += 1;
            if *reference_price > 0.0 {
                let diff = ((close - reference_price) / reference_price).abs();
                max_diff = Some(max_diff.map_or(diff, |current| current.max(diff)));
            }
        }
    }
    let status_and_summary = if matched == 0 {
        (
            CheckStatus::Unavailable,
            format!(
                "no overlapping dates between {} data days and baseline {}",
                last_close.len(),
                reference.range_label()
            ),
        )
    } else if let Some(diff) = max_diff {
        if diff > thresholds.baseline_max_relative_diff {
            (
                CheckStatus::Warning,
                format!(
                    "max relative difference {:.4} exceeds the {:.4} threshold across {matched} matched day(s)",
                    diff, thresholds.baseline_max_relative_diff
                ),
            )
        } else {
            (
                CheckStatus::Ok,
                format!(
                    "max relative difference {:.4} across {matched} matched day(s)",
                    diff
                ),
            )
        }
    } else {
        (
            CheckStatus::Unavailable,
            "baseline has no positive reference prices for the matched days".to_string(),
        )
    };
    BaselineComparison {
        instrument: reference.instrument.clone(),
        source: reference.source_label.clone(),
        status: status_and_summary.0,
        matched_days: matched,
        data_days: last_close.len(),
        max_relative_diff: max_diff.map(|v| format!("{v:.6}")),
        threshold: format!("{:.4}", thresholds.baseline_max_relative_diff),
        summary: status_and_summary.1,
    }
}

fn baseline_metrics(comparison: &BaselineComparison) -> BTreeMap<String, String> {
    let mut metrics = BTreeMap::new();
    metrics.insert("matched_days".to_string(), comparison.matched_days.to_string());
    metrics.insert("data_days".to_string(), comparison.data_days.to_string());
    metrics.insert(
        "max_relative_diff".to_string(),
        comparison
            .max_relative_diff
            .clone()
            .unwrap_or_else(|| "n/a".to_string()),
    );
    metrics.insert("threshold".to_string(), comparison.threshold.clone());
    metrics
}

fn day_of(kind: &DateKind, raw: i64) -> i64 {
    match kind {
        DateKind::Date => raw,
        DateKind::Timestamp { unit, .. } => match unit {
            TimeUnit::Millis => raw.div_euclid(86_400_000),
            TimeUnit::Micros => raw.div_euclid(86_400_000_000),
            TimeUnit::Nanos => raw.div_euclid(86_400_000_000_000),
        },
    }
}

/// Render a timestamp-like range label for tests and diagnostics summaries.
pub fn day_label(kind: &DateKind, raw: i64) -> String {
    let day = day_of(kind, raw);
    format_date(day)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn row(open: f64, high: f64, low: f64, close: f64, spread: f64) -> Row {
        Row {
            time: Some(0),
            open: Some(open),
            high: Some(high),
            low: Some(low),
            close: Some(close),
            spread: Some(spread),
        }
    }

    #[test]
    fn constant_spread_is_flagged() {
        let profile = MarketProfile {
            time_field: "time".into(),
            open_field: "open".into(),
            high_field: "high".into(),
            low_field: "low".into(),
            close_field: "close".into(),
            spread_field: Some("spread".into()),
            instrument: None,
            thresholds: MarketThresholds::default(),
        };
        let rows = vec![row(1.0, 1.1, 0.9, 1.0, 0.1); 20];
        let check = spread_check(&rows, &profile, &profile.thresholds);
        assert_eq!(check.status, CheckStatus::Warning);
        assert!(check.summary.contains("constant"));
    }

    #[test]
    fn open_equal_previous_close_is_flagged() {
        let rows = vec![row(1.0, 1.1, 0.9, 1.0, 0.1); 20];
        let check = open_prev_close_check(&rows, &MarketThresholds::default());
        assert_eq!(check.status, CheckStatus::Warning);
    }

    #[test]
    fn zero_returns_are_flagged() {
        let rows = vec![row(1.0, 1.1, 0.9, 1.0, 0.1); 20];
        let check = return_tail_check(&rows, &MarketThresholds::default());
        assert_eq!(check.status, CheckStatus::Warning);
    }

    #[test]
    fn implausible_ohlc_is_flagged() {
        let rows = vec![row(1.0, 0.8, 1.2, 1.0, 0.1); 5];
        let check = price_plausibility_check(&rows, &MarketThresholds::default());
        assert_eq!(check.status, CheckStatus::Warning);
    }

    #[test]
    fn healthy_series_has_no_warnings() {
        let mut rows = Vec::new();
        let mut price = 1.0;
        for index in 0..100 {
            let open = price + 0.0001 * (index % 3) as f64;
            let close = open + 0.0002 * ((index % 5) as f64 - 2.0);
            rows.push(Row {
                time: Some(index * 60_000),
                open: Some(open),
                high: Some(open.max(close) + 0.0001),
                low: Some(open.min(close) - 0.0001),
                close: Some(close),
                spread: Some(0.0003 + (index % 4) as f64 * 0.00001),
            });
            price = close;
        }
        let profile = MarketProfile {
            time_field: "time".into(),
            open_field: "open".into(),
            high_field: "high".into(),
            low_field: "low".into(),
            close_field: "close".into(),
            spread_field: Some("spread".into()),
            instrument: None,
            thresholds: MarketThresholds::default(),
        };
        assert_eq!(
            spread_check(&rows, &profile, &profile.thresholds).status,
            CheckStatus::Ok
        );
        assert_ne!(
            return_tail_check(&rows, &profile.thresholds).status,
            CheckStatus::Warning
        );
        assert_ne!(
            price_plausibility_check(&rows, &profile.thresholds).status,
            CheckStatus::Warning
        );
    }

    #[test]
    fn profile_validation_rejects_missing_fields_and_bad_ratios() {
        let mut profile = MarketProfile {
            time_field: "".into(),
            open_field: "open".into(),
            high_field: "high".into(),
            low_field: "low".into(),
            close_field: "close".into(),
            spread_field: None,
            instrument: None,
            thresholds: MarketThresholds::default(),
        };
        assert!(profile.validate().is_err());
        profile.time_field = "time".into();
        profile.thresholds.max_zero_return_ratio = 2.0;
        assert!(profile.validate().is_err());
    }

    #[test]
    fn day_buckets() {
        assert_eq!(day_of(&DateKind::Date, 19_723), 19_723);
        assert_eq!(
            day_of(
                &DateKind::Timestamp {
                    unit: TimeUnit::Millis,
                    adjusted_to_utc: true
                },
                86_400_000
            ),
            1
        );
    }

    #[test]
    fn decimal_conversion() {
        assert!((decimal_to_f64(12345, 2) - 123.45).abs() < 1e-9);
    }
}
