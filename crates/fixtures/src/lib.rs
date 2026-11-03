// SPDX-License-Identifier: Apache-2.0
//! Deterministic fixture generator.
//!
//! The generated files are committed under `tests/fixtures` and loaded by the
//! parser tests. Regenerate with:
//!
//! ```text
//! cargo run -p parquet-preview-fixtures -- tests/fixtures
//! ```

use std::fs::{self, File};
use std::io::{self, Write};
use std::path::Path;
use std::sync::Arc;

use arrow_array::{
    ArrayRef, BinaryArray, BooleanArray, Date32Array, Decimal128Array, Float64Array, Int32Array,
    Int64Array, ListArray, NullArray, StringArray, StructArray, TimestampMicrosecondArray,
    TimestampMillisecondArray,
};
use arrow_buffer::{NullBuffer, OffsetBuffer};
use arrow_schema::{DataType, Field, Fields, TimeUnit};
use parquet::arrow::ArrowWriter;
use parquet::basic::Compression;
use parquet::file::properties::{EnabledStatistics, WriterProperties};

pub const TRADES: &str = "trades.parquet";
pub const MULTI_ROW_GROUPS: &str = "multi_row_groups.parquet";
pub const MISSING_STATS: &str = "missing_stats.parquet";
pub const NESTED: &str = "nested.parquet";
pub const CONSTANT_MARKET: &str = "constant_market.parquet";
pub const EMPTY: &str = "empty.parquet";

/// Write every fixture into `dir`, creating it when needed.
pub fn write_all(dir: &Path) -> io::Result<()> {
    fs::create_dir_all(dir)?;
    write_trades(&dir.join(TRADES))?;
    write_multi_row_groups(&dir.join(MULTI_ROW_GROUPS))?;
    write_missing_stats(&dir.join(MISSING_STATS))?;
    write_nested(&dir.join(NESTED))?;
    write_constant_market(&dir.join(CONSTANT_MARKET))?;
    write_empty(&dir.join(EMPTY))?;
    write_codecs(dir)?;
    write_malformed(dir)?;
    Ok(())
}

fn props(max_row_group_size: usize, compression: Compression) -> WriterProperties {
    WriterProperties::builder()
        .set_max_row_group_row_count(Some(max_row_group_size))
        .set_compression(compression)
        .set_statistics_enabled(EnabledStatistics::Chunk)
        .build()
}

fn props_without_stats(max_row_group_size: usize, compression: Compression) -> WriterProperties {
    WriterProperties::builder()
        .set_max_row_group_row_count(Some(max_row_group_size))
        .set_compression(compression)
        .set_statistics_enabled(EnabledStatistics::None)
        .build()
}

fn write_batches(
    path: &Path,
    schema: Arc<arrow_schema::Schema>,
    batches: &[arrow_array::RecordBatch],
    properties: WriterProperties,
) -> io::Result<()> {
    let file = File::create(path)?;
    let mut writer = ArrowWriter::try_new(file, schema, Some(properties))
        .map_err(|e| io::Error::other(e.to_string()))?;
    for batch in batches {
        writer
            .write(batch)
            .map_err(|e| io::Error::other(e.to_string()))?;
    }
    writer
        .close()
        .map_err(|e| io::Error::other(e.to_string()))?;
    Ok(())
}

fn market_schema() -> Arc<arrow_schema::Schema> {
    Arc::new(arrow_schema::Schema::new(vec![
        Field::new(
            "ts",
            DataType::Timestamp(TimeUnit::Millisecond, Some("UTC".into())),
            true,
        ),
        Field::new("symbol", DataType::Utf8, true),
        Field::new("open", DataType::Float64, false),
        Field::new("high", DataType::Float64, false),
        Field::new("low", DataType::Float64, false),
        Field::new("close", DataType::Float64, false),
        Field::new("volume", DataType::Int64, false),
        Field::new("spread", DataType::Float64, true),
        Field::new(
            "meta",
            DataType::Struct(Fields::from(vec![
                Field::new("venue", DataType::Utf8, true),
                Field::new("seq", DataType::Int64, false),
            ])),
            true,
        ),
        Field::new(
            "tags",
            DataType::List(Arc::new(Field::new("item", DataType::Utf8, true))),
            true,
        ),
    ]))
}

/// 60 rows in three row groups, UTC millisecond timestamps, nested struct and
/// list, nullable strings and null timestamps mixed in.
fn write_trades(path: &Path) -> io::Result<()> {
    let rows = 60usize;
    let base = 1_704_067_200_000i64; // 2024-01-01T00:00:00Z
    let ts: Vec<Option<i64>> = (0..rows)
        .map(|i| {
            if i % 17 == 16 {
                None
            } else {
                Some(base + (i as i64) * 300_000)
            }
        })
        .collect();
    let symbols: Vec<Option<&str>> = (0..rows)
        .map(|i| if i % 13 == 12 { None } else { Some("EURUSD") })
        .collect();
    let open: Vec<f64> = (0..rows).map(|i| 1.1000 + (i as f64) * 0.0001).collect();
    let high: Vec<f64> = open.iter().map(|v| v + 0.0004).collect();
    let low: Vec<f64> = open.iter().map(|v| v - 0.0003).collect();
    let close: Vec<f64> = open.iter().map(|v| v + 0.0001).collect();
    let volume: Vec<i64> = (0..rows).map(|i| 100 + i as i64).collect();
    let spread: Vec<Option<f64>> = (0..rows)
        .map(|i| {
            if i % 19 == 18 {
                None
            } else {
                Some(0.0002 + (i % 5) as f64 * 0.00001)
            }
        })
        .collect();

    let venue: Vec<Option<&str>> = (0..rows)
        .map(|i| if i % 11 == 10 { None } else { Some("SIM") })
        .collect();
    let seq: Vec<i64> = (0..rows).map(|i| i as i64).collect();
    let meta = StructArray::new(
        Fields::from(vec![
            Field::new("venue", DataType::Utf8, true),
            Field::new("seq", DataType::Int64, false),
        ]),
        vec![
            Arc::new(StringArray::from(venue)) as ArrayRef,
            Arc::new(Int64Array::from(seq)) as ArrayRef,
        ],
        Some(NullBuffer::from(
            (0..rows).map(|i| i % 23 != 22).collect::<Vec<_>>(),
        )),
    );

    let mut tag_offsets: Vec<i32> = vec![0];
    let mut tag_values: Vec<Option<&str>> = Vec::new();
    for i in 0..rows {
        if i % 7 == 0 {
            tag_values.push(Some("fx"));
            tag_values.push(Some("sim"));
            tag_offsets.push(tag_values.len() as i32);
        } else if i % 5 == 0 {
            tag_values.push(None);
            tag_offsets.push(tag_values.len() as i32);
        } else {
            tag_offsets.push(tag_values.len() as i32);
        }
    }
    let tags = ListArray::new(
        Arc::new(Field::new("item", DataType::Utf8, true)),
        OffsetBuffer::new(tag_offsets.into()),
        Arc::new(StringArray::from(tag_values)),
        Some(NullBuffer::from(
            (0..rows).map(|i| i % 29 != 28).collect::<Vec<_>>(),
        )),
    );

    let batch = arrow_array::RecordBatch::try_new(
        market_schema(),
        vec![
            Arc::new(TimestampMillisecondArray::from(ts).with_timezone("UTC")),
            Arc::new(StringArray::from(symbols)),
            Arc::new(Float64Array::from(open)),
            Arc::new(Float64Array::from(high)),
            Arc::new(Float64Array::from(low)),
            Arc::new(Float64Array::from(close)),
            Arc::new(Int64Array::from(volume)),
            Arc::new(Float64Array::from(spread)),
            Arc::new(meta),
            Arc::new(tags),
        ],
    )
    .map_err(|e| io::Error::other(e.to_string()))?;

    write_batches(
        path,
        market_schema(),
        &[batch],
        props(20, Compression::SNAPPY),
    )
}

/// 100 rows across five row groups with date and timestamp columns.
fn write_multi_row_groups(path: &Path) -> io::Result<()> {
    let rows = 100usize;
    let base_day = 19_723i32; // 2024-01-01
    let base_ms = 1_704_067_200_000i64;
    let schema = Arc::new(arrow_schema::Schema::new(vec![
        Field::new("day", DataType::Date32, false),
        Field::new(
            "ts",
            DataType::Timestamp(TimeUnit::Microsecond, Some("UTC".into())),
            false,
        ),
        Field::new("value", DataType::Int32, false),
    ]));
    let batch = arrow_array::RecordBatch::try_new(
        schema.clone(),
        vec![
            Arc::new(Date32Array::from(
                (0..rows)
                    .map(|i| base_day + (i / 10) as i32)
                    .collect::<Vec<_>>(),
            )),
            Arc::new(
                TimestampMicrosecondArray::from(
                    (0..rows)
                        .map(|i| base_ms * 1_000 + (i as i64) * 3_600_000_000)
                        .collect::<Vec<_>>(),
                )
                .with_timezone("UTC"),
            ),
            Arc::new(Int32Array::from(
                (0..rows).map(|i| i as i32).collect::<Vec<_>>(),
            )),
        ],
    )
    .map_err(|e| io::Error::other(e.to_string()))?;
    write_batches(
        path,
        schema,
        &[batch],
        props(20, Compression::ZSTD(Default::default())),
    )
}

/// Two row groups with statistics disabled, so bounds cannot be promised.
fn write_missing_stats(path: &Path) -> io::Result<()> {
    let schema = Arc::new(arrow_schema::Schema::new(vec![
        Field::new(
            "ts",
            DataType::Timestamp(TimeUnit::Millisecond, Some("UTC".into())),
            false,
        ),
        Field::new("value", DataType::Float64, true),
    ]));
    let batch = arrow_array::RecordBatch::try_new(
        schema.clone(),
        vec![
            Arc::new(
                TimestampMillisecondArray::from(
                    (0..30)
                        .map(|i| 1_704_067_200_000i64 + i * 60_000)
                        .collect::<Vec<_>>(),
                )
                .with_timezone("UTC"),
            ),
            Arc::new(Float64Array::from(
                (0..30).map(|i| i as f64 + 0.5).collect::<Vec<_>>(),
            )),
        ],
    )
    .map_err(|e| io::Error::other(e.to_string()))?;
    write_batches(
        path,
        schema,
        &[batch],
        props_without_stats(15, Compression::SNAPPY),
    )
}

/// Nullable nested values, decimal, binary, boolean, date, timestamp without a
/// timezone and a null column.
fn write_nested(path: &Path) -> io::Result<()> {
    let rows = 6usize;
    let schema = Arc::new(arrow_schema::Schema::new(vec![
        Field::new("id", DataType::Int32, false),
        Field::new(
            "payload",
            DataType::Struct(Fields::from(vec![
                Field::new("kind", DataType::Utf8, true),
                Field::new(
                    "flags",
                    DataType::List(Arc::new(Field::new("item", DataType::Boolean, true))),
                    true,
                ),
            ])),
            true,
        ),
        Field::new("amount", DataType::Decimal128(12, 2), true),
        Field::new("blob", DataType::Binary, true),
        Field::new("day", DataType::Date32, true),
        Field::new(
            "local_ts",
            DataType::Timestamp(TimeUnit::Microsecond, None),
            true,
        ),
        Field::new("nothing", DataType::Null, true),
    ]));

    let kind: Vec<Option<&str>> = vec![
        Some("alpha"),
        None,
        Some("beta"),
        Some("gamma"),
        None,
        Some("delta"),
    ];
    let mut flag_offsets: Vec<i32> = vec![0];
    let mut flag_values: Vec<Option<bool>> = Vec::new();
    for i in 0..rows {
        if i % 2 == 0 {
            flag_values.push(Some(true));
            flag_values.push(Some(false));
        }
        flag_offsets.push(flag_values.len() as i32);
    }
    let flags = ListArray::new(
        Arc::new(Field::new("item", DataType::Boolean, true)),
        OffsetBuffer::new(flag_offsets.into()),
        Arc::new(BooleanArray::from(flag_values)),
        None,
    );
    let payload = StructArray::new(
        Fields::from(vec![
            Field::new("kind", DataType::Utf8, true),
            Field::new(
                "flags",
                DataType::List(Arc::new(Field::new("item", DataType::Boolean, true))),
                true,
            ),
        ]),
        vec![
            Arc::new(StringArray::from(kind)) as ArrayRef,
            Arc::new(flags) as ArrayRef,
        ],
        Some(NullBuffer::from(vec![true, false, true, true, true, true])),
    );
    let amounts: Vec<Option<i128>> = vec![
        Some(123_45),
        None,
        Some(-9_99),
        Some(1_000_00),
        Some(0),
        None,
    ];
    let amount = Decimal128Array::from(amounts)
        .with_precision_and_scale(12, 2)
        .map_err(|e| io::Error::other(e.to_string()))?;
    let blobs: Vec<Option<&[u8]>> = vec![
        Some(&[0xde, 0xad, 0xbe, 0xef]),
        None,
        Some(&[0x00, 0x01, 0x02]),
        Some(&[0xff]),
        None,
        Some(&[0x41, 0x42]),
    ];
    let days: Vec<Option<i32>> = vec![
        Some(19_723),
        None,
        Some(19_724),
        Some(19_726),
        None,
        Some(19_730),
    ];
    let local_ts: Vec<Option<i64>> = vec![
        Some(0),
        Some(1_500_000),
        None,
        Some(-1_500_000),
        Some(1_704_067_200_000_000),
        None,
    ];

    let batch = arrow_array::RecordBatch::try_new(
        schema.clone(),
        vec![
            Arc::new(Int32Array::from((0..rows as i32).collect::<Vec<_>>())),
            Arc::new(payload),
            Arc::new(amount),
            Arc::new(BinaryArray::from(blobs)),
            Arc::new(Date32Array::from(days)),
            Arc::new(TimestampMicrosecondArray::from(local_ts)),
            Arc::new(NullArray::new(rows)),
        ],
    )
    .map_err(|e| io::Error::other(e.to_string()))?;
    write_batches(path, schema, &[batch], props(10, Compression::SNAPPY))
}

/// A deliberately suspicious market series: constant spread, every open equal
/// to the previous close and a flat close price.
fn write_constant_market(path: &Path) -> io::Result<()> {
    let rows = 40usize;
    let base = 1_704_067_200_000i64;
    let schema = Arc::new(arrow_schema::Schema::new(vec![
        Field::new(
            "time",
            DataType::Timestamp(TimeUnit::Millisecond, Some("UTC".into())),
            false,
        ),
        Field::new("open", DataType::Float64, false),
        Field::new("high", DataType::Float64, false),
        Field::new("low", DataType::Float64, false),
        Field::new("close", DataType::Float64, false),
        Field::new("spread", DataType::Float64, false),
    ]));
    let batch = arrow_array::RecordBatch::try_new(
        schema.clone(),
        vec![
            Arc::new(
                TimestampMillisecondArray::from(
                    (0..rows)
                        .map(|i| base + i as i64 * 60_000)
                        .collect::<Vec<_>>(),
                )
                .with_timezone("UTC"),
            ),
            Arc::new(Float64Array::from(vec![1.1000; rows])),
            Arc::new(Float64Array::from(vec![1.1005; rows])),
            Arc::new(Float64Array::from(vec![1.0995; rows])),
            Arc::new(Float64Array::from(vec![1.1000; rows])),
            Arc::new(Float64Array::from(vec![0.0010; rows])),
        ],
    )
    .map_err(|e| io::Error::other(e.to_string()))?;
    write_batches(path, schema, &[batch], props(20, Compression::SNAPPY))
}

fn write_empty(path: &Path) -> io::Result<()> {
    let schema = Arc::new(arrow_schema::Schema::new(vec![Field::new(
        "ts",
        DataType::Timestamp(TimeUnit::Millisecond, Some("UTC".into())),
        false,
    )]));
    let batch = arrow_array::RecordBatch::new_empty(schema.clone());
    write_batches(path, schema, &[batch], props(10, Compression::SNAPPY))
}

fn write_codecs(dir: &Path) -> io::Result<()> {
    let codecs = [
        ("codec_snappy.parquet", Compression::SNAPPY),
        ("codec_gzip.parquet", Compression::GZIP(Default::default())),
        ("codec_zstd.parquet", Compression::ZSTD(Default::default())),
        (
            "codec_brotli.parquet",
            Compression::BROTLI(Default::default()),
        ),
        ("codec_lz4.parquet", Compression::LZ4_RAW),
    ];
    let schema = Arc::new(arrow_schema::Schema::new(vec![
        Field::new("id", DataType::Int64, false),
        Field::new("text", DataType::Utf8, true),
    ]));
    for (name, codec) in codecs {
        let batch = arrow_array::RecordBatch::try_new(
            schema.clone(),
            vec![
                Arc::new(Int64Array::from((0..10i64).collect::<Vec<_>>())),
                Arc::new(StringArray::from(
                    (0..10).map(|i| format!("row {i}")).collect::<Vec<_>>(),
                )),
            ],
        )
        .map_err(|e| io::Error::other(e.to_string()))?;
        write_batches(&dir.join(name), schema.clone(), &[batch], props(10, codec))?;
    }
    Ok(())
}

fn write_malformed(dir: &Path) -> io::Result<()> {
    let malformed = dir.join("malformed");
    fs::create_dir_all(&malformed)?;
    let source = dir.join(TRADES);
    let bytes = fs::read(&source)?;

    fs::write(
        malformed.join("truncated.parquet"),
        &bytes[..bytes.len() - 20],
    )?;

    let mut bad_magic = bytes.clone();
    let len = bad_magic.len();
    bad_magic[len - 4..].copy_from_slice(b"XXXX");
    fs::write(malformed.join("bad_magic.parquet"), &bad_magic)?;

    let mut bad_header = bytes.clone();
    bad_header[0] = b'X';
    fs::write(malformed.join("bad_header.parquet"), &bad_header)?;

    let mut footer_too_big = bytes.clone();
    let len = footer_too_big.len();
    footer_too_big[len - 8..len - 4].copy_from_slice(&0xFFFF_FF00u32.to_le_bytes());
    fs::write(malformed.join("footer_too_big.parquet"), &footer_too_big)?;

    let mut encrypted = bytes.clone();
    let len = encrypted.len();
    encrypted[0..4].copy_from_slice(b"PARE");
    encrypted[len - 4..].copy_from_slice(b"PARE");
    fs::write(malformed.join("encrypted.parquet"), &encrypted)?;

    fs::write(malformed.join("empty.parquet"), b"")?;

    let mut short_tail = bytes;
    short_tail.truncate(7);
    fs::write(malformed.join("short.parquet"), short_tail)?;
    Ok(())
}

/// Write a small baseline CSV used by the market tests.
pub fn write_baseline(path: &Path) -> io::Result<()> {
    let mut file = File::create(path)?;
    writeln!(file, "instrument,date,price")?;
    for (day, price) in [(19_723, "1.1001"), (19_724, "1.1002"), (19_725, "1.1003")] {
        let (y, m, d) = civil(day);
        writeln!(file, "EURUSD,{y:04}-{m:02}-{d:02},{price}")?;
    }
    Ok(())
}

fn civil(days: i64) -> (i64, u32, u32) {
    let z = days + 719_468;
    let era = if z >= 0 { z } else { z - 146_096 } / 146_097;
    let doe = z - era * 146_097;
    let yoe = (doe - doe / 1_460 + doe / 36_524 - doe / 146_096) / 365;
    let y = yoe + era * 400;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let d = doy - (153 * mp + 2) / 5 + 1;
    let m = if mp < 10 { mp + 3 } else { mp - 9 };
    (if m <= 2 { y + 1 } else { y }, m as u32, d as u32)
}

/// Path to the committed fixture directory for a crate inside this workspace.
pub fn fixture_dir(crate_manifest_dir: &str) -> std::path::PathBuf {
    std::path::Path::new(crate_manifest_dir)
        .join("..")
        .join("..")
        .join("tests")
        .join("fixtures")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn writes_all_fixtures() {
        let dir = std::env::temp_dir().join(format!(
            "parquet-preview-fixtures-test-{}",
            std::process::id()
        ));
        let _ = fs::remove_dir_all(&dir);
        write_all(&dir).unwrap();
        for name in [
            TRADES,
            MULTI_ROW_GROUPS,
            MISSING_STATS,
            NESTED,
            CONSTANT_MARKET,
            EMPTY,
            "codec_snappy.parquet",
            "codec_gzip.parquet",
            "codec_zstd.parquet",
            "codec_brotli.parquet",
            "codec_lz4.parquet",
            "malformed/truncated.parquet",
        ] {
            let path = dir.join(name);
            assert!(path.is_file(), "missing {name}");
            assert!(fs::metadata(&path).unwrap().len() > 0, "empty {name}");
        }
        write_baseline(&dir.join("baseline.csv")).unwrap();
        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn civil_matches_known_epoch() {
        assert_eq!(civil(0), (1970, 1, 1));
        assert_eq!(civil(19_723), (2024, 1, 1));
    }
}

/// Deterministic paging/search fixture, emitted in bounded batches.
pub fn write_browse(path: &Path, rows: usize) -> io::Result<()> {
    let schema = Arc::new(arrow_schema::Schema::new(vec![
        Field::new("id", DataType::Int32, false),
        Field::new("note", DataType::Utf8, false),
        Field::new("payload", DataType::Utf8, false),
    ]));
    let properties = WriterProperties::builder()
        .set_max_row_group_row_count(Some(500))
        .set_compression(Compression::UNCOMPRESSED)
        .set_dictionary_enabled(false)
        .build();
    let mut writer = ArrowWriter::try_new(File::create(path)?, schema.clone(), Some(properties))
        .map_err(|e| io::Error::other(e.to_string()))?;
    for start in (0..rows).step_by(500) {
        let end = (start + 500).min(rows);
        let ids = Int32Array::from((start..end).map(|i| i as i32).collect::<Vec<_>>());
        let notes = StringArray::from(
            (start..end)
                .map(|i| if i == 450 { "NEEDLE.*" } else { "ordinary" })
                .collect::<Vec<_>>(),
        );
        let payloads = StringArray::from(
            (start..end)
                .map(|i| format!("{i:08}{}", "x".repeat(1024)))
                .collect::<Vec<_>>(),
        );
        let batch = arrow_array::RecordBatch::try_new(
            schema.clone(),
            vec![Arc::new(ids), Arc::new(notes), Arc::new(payloads)],
        )
        .map_err(|e| io::Error::other(e.to_string()))?;
        writer
            .write(&batch)
            .map_err(|e| io::Error::other(e.to_string()))?;
    }
    writer
        .close()
        .map_err(|e| io::Error::other(e.to_string()))?;
    Ok(())
}
