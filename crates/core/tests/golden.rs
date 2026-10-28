// SPDX-License-Identifier: Apache-2.0
//! Golden text snapshots for the rendered preview.
//!
//! Regenerate with `UPDATE_GOLDEN=1 cargo test -p parquet-preview-core --test golden`.

use std::path::PathBuf;

use parquet_preview_core::baseline::BaselineSpec;
use parquet_preview_core::market::{MarketProfile, MarketThresholds};
use parquet_preview_core::{Limits, PreviewOptions, Session};
use parquet_preview_fixtures as fixtures;

fn fixture(name: &str) -> PathBuf {
    fixtures::fixture_dir(env!("CARGO_MANIFEST_DIR")).join(name)
}

fn golden_dir() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("tests")
        .join("golden")
}

fn check(name: &str, actual: &str) {
    let path = golden_dir().join(name);
    if std::env::var("UPDATE_GOLDEN").as_deref() == Ok("1") {
        std::fs::create_dir_all(golden_dir()).unwrap();
        std::fs::write(&path, actual).unwrap();
        return;
    }
    let expected = std::fs::read_to_string(&path).unwrap_or_else(|e| {
        panic!(
            "cannot read {}: {e}; run with UPDATE_GOLDEN=1",
            path.display()
        )
    });
    // Elapsed time is measured at runtime and is not stable across runs.
    fn normalize(text: &str) -> String {
        text.lines()
            .map(|line| {
                if line.starts_with("complete:") && line.ends_with(" ms") {
                    if let Some((prefix, _)) = line.rsplit_once(", ") {
                        return format!("{prefix}, <elapsed> ms\n");
                    }
                }
                format!("{line}\n")
            })
            .collect()
    }
    assert_eq!(
        normalize(actual),
        normalize(&expected),
        "golden mismatch: {}",
        path.display()
    );
}

#[test]
fn trades_preview_golden() {
    let session = Session::open_path(fixture(fixtures::TRADES), &Limits::default()).unwrap();
    let options = PreviewOptions {
        display_name: Some("trades.parquet".into()),
        sample_limit: 5,
        ..Default::default()
    };
    check("trades.txt", &session.render_preview(&options).unwrap());
}

#[test]
fn nested_sample_golden() {
    let session = Session::open_path(fixture(fixtures::NESTED), &Limits::default()).unwrap();
    let options = PreviewOptions {
        display_name: Some("nested.parquet".into()),
        sample_limit: 6,
        ..Default::default()
    };
    check("nested.txt", &session.render_preview(&options).unwrap());
}

#[test]
fn missing_stats_scan_golden() {
    let session = Session::open_path(fixture(fixtures::MISSING_STATS), &Limits::default()).unwrap();
    let options = PreviewOptions {
        display_name: Some("missing_stats.parquet".into()),
        sample_limit: 3,
        scan: true,
        ..Default::default()
    };
    check(
        "missing_stats_scan.txt",
        &session.render_preview(&options).unwrap(),
    );
}

#[test]
fn constant_market_diagnostics_golden() {
    let session =
        Session::open_path(fixture(fixtures::CONSTANT_MARKET), &Limits::default()).unwrap();
    let profile = MarketProfile {
        time_field: "time".into(),
        open_field: "open".into(),
        high_field: "high".into(),
        low_field: "low".into(),
        close_field: "close".into(),
        spread_field: Some("spread".into()),
        instrument: Some("EURUSD".into()),
        thresholds: MarketThresholds::default(),
    };
    let options = PreviewOptions {
        display_name: Some("constant_market.parquet".into()),
        sample_limit: 3,
        scan: true,
        market_profile: Some(profile),
        baseline: Some(BaselineSpec {
            path: fixture("baseline.csv").to_string_lossy().to_string(),
            instrument: Some("EURUSD".into()),
            licence: Some("synthetic test baseline".into()),
        }),
        diagnostic_rows: 1000,
        ..Default::default()
    };
    check(
        "constant_market.txt",
        &session.render_preview(&options).unwrap(),
    );
}
