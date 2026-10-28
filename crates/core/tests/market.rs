// SPDX-License-Identifier: Apache-2.0
//! Fixture tests for optional market diagnostics.

use parquet_preview_core::baseline::BaselineSpec;
use parquet_preview_core::market::{CheckStatus, MarketProfile, MarketThresholds};
use parquet_preview_core::{CancelToken, Limits, Session};
use parquet_preview_fixtures as fixtures;

fn fixture_dir() -> std::path::PathBuf {
    fixtures::fixture_dir(env!("CARGO_MANIFEST_DIR"))
}

fn profile() -> MarketProfile {
    MarketProfile {
        time_field: "ts".into(),
        open_field: "open".into(),
        high_field: "high".into(),
        low_field: "low".into(),
        close_field: "close".into(),
        spread_field: Some("spread".into()),
        instrument: Some("EURUSD".into()),
        thresholds: MarketThresholds::default(),
    }
}

fn open(name: &str, limits: &Limits) -> Session {
    Session::open_path(fixture_dir().join(name), limits).unwrap()
}

fn check<'a>(
    report: &'a parquet_preview_core::market::MarketReport,
    id: &str,
) -> &'a parquet_preview_core::market::DiagnosticCheck {
    report.checks.iter().find(|c| c.id == id).unwrap()
}

#[test]
fn trades_trigger_open_prev_close_warning() {
    let session = open(fixtures::TRADES, &Limits::default());
    let report = session
        .diagnose(&profile(), None, 1000, &CancelToken::new())
        .unwrap();
    assert_eq!(report.status, CheckStatus::Warning);
    assert_eq!(report.rows_used, 60);
    assert!(report.coverage.contains("not a full scan"));
    assert_eq!(check(&report, "open_prev_close").status, CheckStatus::Warning);
    assert_eq!(check(&report, "spread").status, CheckStatus::Ok);
    assert_eq!(check(&report, "price_plausibility").status, CheckStatus::Ok);
    assert_eq!(check(&report, "baseline").status, CheckStatus::Unavailable);
    assert!(report.time_range.is_some());
    assert!(report
        .notes
        .iter()
        .any(|n| n.contains("not proof of fraud")));
}

#[test]
fn constant_market_triggers_multiple_warnings() {
    let session = open(fixtures::CONSTANT_MARKET, &Limits::default());
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
    let report = session
        .diagnose(&profile, None, 1000, &CancelToken::new())
        .unwrap();
    assert_eq!(report.status, CheckStatus::Warning);
    assert_eq!(check(&report, "spread").status, CheckStatus::Warning);
    assert_eq!(check(&report, "open_prev_close").status, CheckStatus::Warning);
    assert_eq!(check(&report, "return_tails").status, CheckStatus::Warning);
}

#[test]
fn row_cap_and_cancellation_are_reported() {
    let mut limits = Limits::default();
    limits.max_diagnostic_rows = 10;
    let session = open(fixtures::TRADES, &limits);
    let report = session
        .diagnose(&profile(), None, 50_000, &CancelToken::new())
        .unwrap();
    assert_eq!(report.rows_used, 10);
    assert!(report.coverage.contains("first 10 of 60"));

    let cancel = CancelToken::new();
    cancel.cancel();
    let report = session
        .diagnose(&profile(), None, 1000, &cancel)
        .unwrap();
    assert_eq!(report.status, CheckStatus::Unavailable);
    assert!(report.notes.iter().any(|n| n.contains("cancelled")));
}

#[test]
fn invalid_field_mapping_is_unavailable_not_a_crash() {
    let session = open(fixtures::TRADES, &Limits::default());
    let mut bad = profile();
    bad.close_field = "symbol".into();
    let report = session
        .diagnose(&bad, None, 1000, &CancelToken::new())
        .unwrap();
    assert_eq!(report.status, CheckStatus::Unavailable);
    assert!(report.checks[0].summary.contains("not numeric"));

    let mut missing = profile();
    missing.open_field = "does_not_exist".into();
    let report = session
        .diagnose(&missing, None, 1000, &CancelToken::new())
        .unwrap();
    assert_eq!(report.status, CheckStatus::Unavailable);
    assert!(report.checks[0].summary.contains("missing"));
}

#[test]
fn dated_baseline_compares_overlapping_days() {
    let session = open(fixtures::TRADES, &Limits::default());
    let spec = BaselineSpec {
        path: fixture_dir().join("baseline.csv").to_string_lossy().to_string(),
        instrument: Some("EURUSD".into()),
    };
    let report = session
        .diagnose(&profile(), Some(&spec), 1000, &CancelToken::new())
        .unwrap();
    let comparison = report.baseline.clone().unwrap();
    assert_eq!(comparison.status, CheckStatus::Ok);
    assert_eq!(comparison.matched_days, 1);
    assert_eq!(comparison.data_days, 1);
    assert!(comparison.summary.contains("max relative difference"));
    assert_eq!(check(&report, "baseline").status, CheckStatus::Ok);
}

#[test]
fn missing_baseline_file_is_reported_without_failing_preview() {
    let session = open(fixtures::TRADES, &Limits::default());
    let spec = BaselineSpec {
        path: fixture_dir().join("does-not-exist.csv").to_string_lossy().to_string(),
        instrument: Some("EURUSD".into()),
    };
    let report = session
        .diagnose(&profile(), Some(&spec), 1000, &CancelToken::new())
        .unwrap();
    assert_eq!(check(&report, "baseline").status, CheckStatus::Unavailable);
    assert!(check(&report, "baseline").summary.contains("baseline"));
}

#[test]
fn zero_row_file_is_unavailable() {
    let session = open(fixtures::EMPTY, &Limits::default());
    let report = session
        .diagnose(&profile(), None, 1000, &CancelToken::new())
        .unwrap();
    assert_eq!(report.status, CheckStatus::Unavailable);
    assert_eq!(report.rows_used, 0);
}
