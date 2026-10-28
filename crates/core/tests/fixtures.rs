// SPDX-License-Identifier: Apache-2.0
//! Fixture tests for the footer, summary and sample parsers.

use parquet_preview_core::date_bounds::CoverageStatus;
use parquet_preview_core::sample::{CellValue, SampleStatus};
use parquet_preview_core::{Limits, PreviewError, Session};
use parquet_preview_fixtures as fixtures;

fn fixture(name: &str) -> std::path::PathBuf {
    fixtures::fixture_dir(env!("CARGO_MANIFEST_DIR")).join(name)
}

fn open(name: &str) -> Session {
    Session::open_path(fixture(name), &Limits::default()).unwrap()
}

#[test]
fn trades_summary_reports_metadata_counts() {
    let session = open(fixtures::TRADES);
    let summary = session.summary();
    assert_eq!(summary.row_count.value, 60);
    assert!(!summary.row_count.exact);
    assert_eq!(summary.row_groups.total, 3);
    assert_eq!(summary.row_groups.rows_metadata, 60);
    assert_eq!(summary.file.row_group_count, 3);
    assert_eq!(summary.file.leaf_column_count, 11);
    assert!(summary.schema.has_nested_types);
    assert_eq!(summary.schema.nested_date_leaves, 0);
    assert!(summary.file.created_by.is_some());

    let ts = summary
        .date_bounds
        .iter()
        .find(|b| b.column == "ts")
        .expect("timestamp bounds");
    assert_eq!(ts.status, CoverageStatus::Complete);
    assert_eq!(ts.covered_row_groups, 3);
    assert_eq!(ts.rows_covered, 60);
    assert_eq!(ts.min.as_deref(), Some("2024-01-01T00:00:00Z"));
    assert_eq!(ts.max.as_deref(), Some("2024-01-01T04:55:00Z"));
    assert!(!ts.verified_by_scan);
}

#[test]
fn sample_returns_requested_page_and_nested_values() {
    let session = open(fixtures::TRADES);
    let sample = session.sample(0, 5).unwrap();
    assert_eq!(sample.status, SampleStatus::Complete);
    assert_eq!(sample.returned, 5);
    assert_eq!(sample.columns.len(), 10);
    assert_eq!(sample.rows.len(), 5);
    assert_eq!(
        sample.rows[0].values[0],
        CellValue::Timestamp("2024-01-01T00:00:00Z".into())
    );
    assert_eq!(sample.rows[0].values[1], CellValue::Text("EURUSD".into()));
    assert_eq!(sample.rows[0].values[8], CellValue::Struct {
        fields: vec![
            parquet_preview_core::sample::StructFieldValue {
                name: "venue".into(),
                value: CellValue::Text("SIM".into()),
            },
            parquet_preview_core::sample::StructFieldValue {
                name: "seq".into(),
                value: CellValue::Integer(0),
            },
        ],
        omitted: 0,
    });
    assert_eq!(
        sample.rows[0].values[9],
        CellValue::List {
            items: vec![
                CellValue::Text("fx".into()),
                CellValue::Text("sim".into())
            ],
            omitted: 0,
        }
    );
}

#[test]
fn sample_pages_do_not_overlap() {
    let session = open(fixtures::TRADES);
    let first = session.sample(0, 3).unwrap();
    let second = session.sample(3, 3).unwrap();
    assert_eq!(first.rows[0].values[5], CellValue::Float("1.1001".into()));
    assert_eq!(second.rows[0].values[5], CellValue::Float("1.1004".into()));
    assert_eq!(
        second.rows[2].values[0],
        CellValue::Timestamp("2024-01-01T00:25:00Z".into())
    );
}

#[test]
fn sample_handles_null_timestamp_and_truncated_offset() {
    let session = open(fixtures::TRADES);
    let sample = session.sample(16, 2).unwrap();
    assert_eq!(sample.rows[0].values[0], CellValue::Null);
    let beyond = session.sample(1_000, 5).unwrap();
    assert_eq!(beyond.status, SampleStatus::Complete);
    assert_eq!(beyond.returned, 0);
}

#[test]
fn sample_respects_compressed_byte_budget() {
    let mut limits = Limits::default();
    limits.max_sample_compressed_bytes = 1;
    let session = Session::open_path(fixture(fixtures::TRADES), &limits).unwrap();
    let sample = session.sample(0, 5).unwrap();
    assert_eq!(sample.status, SampleStatus::Unavailable);
    assert!(sample.message.contains("compressed bytes"));
}

#[test]
fn sample_respects_decoded_byte_budget() {
    let mut limits = Limits::default();
    limits.max_sample_decoded_bytes = 1;
    limits.max_sample_compressed_bytes = 1024 * 1024;
    let session = Session::open_path(fixture(fixtures::TRADES), &limits).unwrap();
    let sample = session.sample(0, 500).unwrap();
    assert_eq!(sample.status, SampleStatus::Truncated);
    assert!(sample.decoded_bytes <= 1024 * 1024);
}

#[test]
fn missing_statistics_are_not_claimed_complete() {
    let session = open(fixtures::MISSING_STATS);
    let bounds = &session.summary().date_bounds;
    assert_eq!(bounds.len(), 1);
    assert_eq!(bounds[0].status, CoverageStatus::Unavailable);
    assert!(bounds[0].missing_stats_row_groups > 0);
    assert!(bounds[0].min.is_none() && bounds[0].max.is_none());
}

#[test]
fn nested_values_render_safely() {
    let session = open(fixtures::NESTED);
    let sample = session.sample(0, 6).unwrap();
    assert_eq!(sample.status, SampleStatus::Complete);
    assert_eq!(sample.returned, 6);
    assert_eq!(sample.rows[0].values[2], CellValue::Decimal("123.45".into()));
    assert_eq!(
        sample.rows[0].values[3],
        CellValue::Binary {
            bytes: 4,
            preview: "dead beef".into()
        }
    );
    assert_eq!(
        sample.rows[0].values[4],
        CellValue::Date("2024-01-01".into())
    );
    assert_eq!(
        sample.rows[1].values[5],
        CellValue::Timestamp("1970-01-01T00:00:01.5 (unadjusted)".into())
    );
    assert_eq!(sample.rows[0].values[6], CellValue::Null);
    assert_eq!(sample.rows[1].values[1], CellValue::Null);
    assert_eq!(sample.rows[2].values[2], CellValue::Decimal("-9.99".into()));
    let rendered = sample.rows[0].values[1].render();
    assert!(rendered.contains("kind: alpha"), "{rendered}");
    assert!(rendered.contains("flags: [true, false]"), "{rendered}");
    assert_eq!(sample.rows[3].values[1].render(), "{kind: gamma, flags: []}");

    let trades = open(fixtures::TRADES).sample(7, 1).unwrap();
    assert_eq!(
        trades.rows[0].values[9],
        CellValue::List {
            items: vec![CellValue::Text("fx".into()), CellValue::Text("sim".into())],
            omitted: 0,
        }
    );
}

#[test]
fn all_compression_codecs_decode() {
    for name in [
        "codec_snappy.parquet",
        "codec_gzip.parquet",
        "codec_zstd.parquet",
        "codec_brotli.parquet",
        "codec_lz4.parquet",
    ] {
        let session = open(name);
        let sample = session.sample(0, 10).unwrap();
        assert_eq!(sample.status, SampleStatus::Complete, "{name}");
        assert_eq!(sample.returned, 10, "{name}");
        assert_eq!(
            sample.rows[9].values[1],
            CellValue::Text("row 9".into()),
            "{name}"
        );
    }
}

#[test]
fn empty_file_is_not_an_error() {
    let session = open(fixtures::EMPTY);
    assert_eq!(session.summary().row_count.value, 0);
    assert_eq!(session.summary().date_bounds[0].status, CoverageStatus::Unavailable);
    let sample = session.sample(0, 10).unwrap();
    assert_eq!(sample.status, SampleStatus::Complete);
    assert_eq!(sample.returned, 0);
}

#[test]
fn malformed_inputs_are_rejected_with_specific_errors() {
    let cases = [
        ("malformed/empty.parquet", "not a parquet"),
        ("malformed/short.parquet", "not a parquet"),
        ("malformed/truncated.parquet", "not a parquet"),
        ("malformed/bad_magic.parquet", "not a parquet"),
        ("malformed/bad_header.parquet", "not a parquet"),
        ("malformed/footer_too_big.parquet", "footer"),
        ("malformed/encrypted.parquet", "encrypted"),
    ];
    for (name, needle) in cases {
        let error = Session::open_path(fixture(name), &Limits::default()).unwrap_err();
        match error {
            PreviewError::NotParquet(_)
            | PreviewError::FooterTooLarge { .. }
            | PreviewError::MalformedFooter(_)
            | PreviewError::Encrypted => {}
            other => panic!("{name}: unexpected error {other}"),
        }
        let text = format!(
            "{}",
            Session::open_path(fixture(name), &Limits::default())
                .map(|_| String::new())
                .unwrap_or_else(|e| e.to_string())
                .to_lowercase()
        );
        assert!(text.contains(needle), "{name}: {text}");
    }
}

#[test]
fn file_size_limit_is_enforced_before_parsing() {
    let mut limits = Limits::default();
    limits.max_file_bytes = 100;
    let error = Session::open_path(fixture(fixtures::TRADES), &limits).unwrap_err();
    assert!(matches!(error, PreviewError::FileTooLarge { .. }));
}

#[test]
fn schema_and_row_group_display_caps_are_reported() {
    let mut limits = Limits::default();
    limits.max_row_groups = 1;
    limits.max_columns = 2;
    let session = Session::open_path(fixture(fixtures::TRADES), &limits).unwrap();
    let summary = session.summary();
    assert_eq!(summary.row_groups.total, 3);
    assert_eq!(summary.row_groups.listed.len(), 1);
    assert_eq!(summary.row_groups.omitted, 2);
    assert_eq!(summary.schema.omitted_leaves, 9);
    assert!(summary
        .notices
        .iter()
        .any(|n| n.contains("row-group display capped")));
    assert!(summary
        .notices
        .iter()
        .any(|n| n.contains("schema display capped")));
}

#[test]
fn inspection_never_modifies_the_input_file() {
    let path = fixture(fixtures::TRADES);
    let before = std::fs::read(&path).unwrap();
    let session = Session::open_path(&path, &Limits::default()).unwrap();
    let _ = session.sample(0, 10).unwrap();
    let report = session.scan(&parquet_preview_core::CancelToken::new()).unwrap();
    assert_eq!(report.exact_row_count, Some(60));
    let after = std::fs::read(&path).unwrap();
    assert_eq!(before, after);
}

#[test]
fn market_sample_has_expected_nulls_and_varies() {
    let session = open(fixtures::TRADES);
    let sample = session.sample(0, 60).unwrap();
    assert_eq!(sample.returned, 60);
    let null_times = sample
        .rows
        .iter()
        .filter(|r| r.values[0] == CellValue::Null)
        .count();
    assert_eq!(null_times, 3);
    let null_spreads = sample
        .rows
        .iter()
        .filter(|r| r.values[7] == CellValue::Null)
        .count();
    assert_eq!(null_spreads, 3);
}
