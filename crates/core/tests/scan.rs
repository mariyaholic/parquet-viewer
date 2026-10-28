// SPDX-License-Identifier: Apache-2.0
//! Fixture tests for the optional full scan.

use parquet_preview_core::date_bounds::CoverageStatus;
use parquet_preview_core::{CancelToken, Limits, ScanStatus, Session};
use parquet_preview_fixtures as fixtures;

fn fixture(name: &str) -> std::path::PathBuf {
    fixtures::fixture_dir(env!("CARGO_MANIFEST_DIR")).join(name)
}

fn open(name: &str, limits: &Limits) -> Session {
    Session::open_path(fixture(name), limits).unwrap()
}

#[test]
fn complete_scan_verifies_count_and_bounds() {
    let session = open(fixtures::MULTI_ROW_GROUPS, &Limits::default());
    let report = session.scan(&CancelToken::new()).unwrap();
    assert_eq!(report.status, ScanStatus::Complete);
    assert_eq!(report.rows_scanned, 100);
    assert_eq!(report.exact_row_count, Some(100));
    assert_eq!(report.row_groups_total, 5);
    assert_eq!(report.row_groups_scanned, 5);
    assert!(report.skipped_row_groups.is_empty());
    assert!(report.decoded_bytes > 0);
    let day = &report.date_bounds[0];
    assert_eq!(day.column, "day");
    assert_eq!(day.status, CoverageStatus::Complete);
    assert!(day.verified_by_scan);
    assert_eq!(day.min.as_deref(), Some("2024-01-01"));
    assert_eq!(day.max.as_deref(), Some("2024-01-10"));
    assert_eq!(day.rows_covered, 100);
}

#[test]
fn scan_fills_bounds_missing_statistics() {
    let session = open(fixtures::MISSING_STATS, &Limits::default());
    assert_eq!(
        session.summary().date_bounds[0].status,
        CoverageStatus::Unavailable
    );
    let report = session.scan(&CancelToken::new()).unwrap();
    assert_eq!(report.status, ScanStatus::Complete);
    assert_eq!(report.exact_row_count, Some(30));
    let bounds = &report.date_bounds[0];
    assert_eq!(bounds.status, CoverageStatus::Complete);
    assert!(bounds.verified_by_scan);
    assert_eq!(bounds.min.as_deref(), Some("2024-01-01T00:00:00Z"));
    assert_eq!(bounds.max.as_deref(), Some("2024-01-01T00:29:00Z"));
}

#[test]
fn row_budget_makes_scan_incomplete() {
    let mut limits = Limits::default();
    limits.max_scan_rows = 10;
    let session = open(fixtures::MULTI_ROW_GROUPS, &limits);
    let report = session.scan(&CancelToken::new()).unwrap();
    assert_eq!(report.status, ScanStatus::Incomplete);
    assert_eq!(report.exact_row_count, None);
    assert!(report.rows_scanned >= 10);
    assert!(report.message.contains("row budget"));
    assert_eq!(report.date_bounds[0].status, CoverageStatus::Partial);
}

#[test]
fn compressed_budget_skips_row_groups() {
    let mut limits = Limits::default();
    limits.max_scan_compressed_bytes = 1;
    let session = open(fixtures::MULTI_ROW_GROUPS, &limits);
    let report = session.scan(&CancelToken::new()).unwrap();
    assert_eq!(report.status, ScanStatus::Incomplete);
    assert!(report.message.contains("compressed"));
    assert_eq!(report.skipped_row_groups.len(), 5);
    assert_eq!(report.exact_row_count, None);
}

#[test]
fn pre_cancelled_scan_does_no_work() {
    let session = open(fixtures::MULTI_ROW_GROUPS, &Limits::default());
    let cancel = CancelToken::new();
    cancel.cancel();
    let report = session.scan(&cancel).unwrap();
    assert_eq!(report.status, ScanStatus::Cancelled);
    assert_eq!(report.rows_scanned, 0);
    assert!(report.date_bounds.is_empty());
}

#[test]
fn time_budget_stops_long_scan() {
    let mut limits = Limits::default();
    limits.max_scan_millis = 0;
    limits.max_scan_rows = u64::MAX;
    let session = open(fixtures::MULTI_ROW_GROUPS, &limits);
    let report = session.scan(&CancelToken::new()).unwrap();
    // A zero budget may still complete if the first batch arrives before the
    // clock is checked on a very small file; either way it must not claim
    // exactness without completing.
    if report.status == ScanStatus::Incomplete {
        assert_eq!(report.exact_row_count, None);
        assert!(report.message.contains("time budget"));
    }
}

#[test]
fn scan_empty_file_reports_zero_rows() {
    let session = open(fixtures::EMPTY, &Limits::default());
    let report = session.scan(&CancelToken::new()).unwrap();
    assert_eq!(report.status, ScanStatus::Complete);
    assert_eq!(report.exact_row_count, Some(0));
    assert_eq!(report.rows_scanned, 0);
}
