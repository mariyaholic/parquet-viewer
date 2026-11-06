// SPDX-License-Identifier: Apache-2.0
//! Protocol-level tests for the worker state machine, using in-memory streams.

use std::io::Cursor;

use parquet_preview_core::market::{MarketProfile, MarketThresholds};
use parquet_preview_core::{PreviewOptions, Settings};
use parquet_preview_fixtures as fixtures;
use parquet_preview_protocol::{
    decode_control, read_frame, write_control, write_stream_chunk, ErrorCode, FrameLimits, Request,
    Response,
};
use parquet_preview_worker::{serve, WorkerConfig};

fn fixture(name: &str) -> std::path::PathBuf {
    fixtures::fixture_dir(env!("CARGO_MANIFEST_DIR")).join(name)
}

#[derive(Default)]
struct Driver {
    input: Vec<u8>,
}

impl Driver {
    fn send(&mut self, request: &Request) {
        write_control(&mut self.input, request).unwrap();
    }

    fn chunk(&mut self, bytes: &[u8]) {
        write_stream_chunk(&mut self.input, bytes).unwrap();
    }

    fn stream_file(&mut self, name: &str) -> u64 {
        let bytes = std::fs::read(fixture(name)).unwrap();
        self.send(&Request::OpenStream {
            declared_size: bytes.len() as u64,
        });
        for chunk in bytes.chunks(64 * 1024) {
            self.chunk(chunk);
        }
        bytes.len() as u64
    }

    fn run(self, config: WorkerConfig) -> Vec<Response> {
        let mut output = Vec::new();
        serve(Cursor::new(self.input), &mut output, config).unwrap();
        let mut reader = Cursor::new(output);
        let mut responses = Vec::new();
        while let Some(frame) = read_frame(&mut reader, &FrameLimits::default()).unwrap() {
            responses.push(decode_control::<Response>(frame).unwrap());
        }
        responses
    }

    fn run_default(self) -> Vec<Response> {
        self.run(WorkerConfig::default())
    }
}

fn hello(driver: &mut Driver) {
    driver.send(&Request::Hello {
        protocol_version: parquet_preview_protocol::PROTOCOL_VERSION,
        client: Some("worker-test".into()),
    });
}

fn market_profile() -> MarketProfile {
    MarketProfile {
        time_field: "time".into(),
        open_field: "open".into(),
        high_field: "high".into(),
        low_field: "low".into(),
        close_field: "close".into(),
        spread_field: Some("spread".into()),
        instrument: Some("EURUSD".into()),
        thresholds: MarketThresholds::default(),
    }
}

#[test]
fn handshake_stream_inspect_sample_render_shutdown() {
    let mut driver = Driver::default();
    hello(&mut driver);
    let size = driver.stream_file(fixtures::TRADES);
    driver.send(&Request::Inspect);
    driver.send(&Request::Sample {
        offset: 0,
        limit: 3,
    });
    driver.send(&Request::Render {
        options: PreviewOptions {
            sample_limit: 2,
            ..Default::default()
        },
    });
    driver.send(&Request::About);
    driver.send(&Request::Shutdown);

    let responses = driver.run_default();
    assert!(matches!(responses[0], Response::Welcome { .. }));
    assert_eq!(responses[1], Response::StreamAccepted { bytes: size });
    match &responses[2] {
        Response::Summary { summary } => {
            assert_eq!(summary.row_count.value, 60);
            assert_eq!(summary.row_groups.total, 3);
        }
        other => panic!("unexpected {other:?}"),
    }
    match &responses[3] {
        Response::Sample { sample } => {
            assert_eq!(sample.returned, 3);
            assert_eq!(sample.columns.len(), 10);
        }
        other => panic!("unexpected {other:?}"),
    }
    match &responses[4] {
        Response::Preview { report, text } => {
            assert_eq!(report.summary.row_count.value, 60);
            assert!(text.contains("Made by Maria"));
            assert!(text.contains("Rows 0-1 of 60"));
        }
        other => panic!("unexpected {other:?}"),
    }
    assert!(matches!(&responses[5], Response::About { text } if text.contains("Made by Maria")));
    assert_eq!(responses[6], Response::Bye);
}

#[test]
fn version_mismatch_is_rejected_and_closed() {
    let mut driver = Driver::default();
    driver.send(&Request::Hello {
        protocol_version: 99,
        client: None,
    });
    let responses = driver.run_default();
    assert!(matches!(
        &responses[0],
        Response::Error {
            code: ErrorCode::ProtocolMismatch,
            ..
        }
    ));
    assert_eq!(responses[1], Response::Bye);
}

#[test]
fn first_request_must_be_hello() {
    let mut driver = Driver::default();
    driver.send(&Request::About);
    let responses = driver.run_default();
    assert!(matches!(
        &responses[0],
        Response::Error {
            code: ErrorCode::ProtocolError,
            ..
        }
    ));
}

#[test]
fn sample_without_file_is_rejected() {
    let mut driver = Driver::default();
    hello(&mut driver);
    driver.send(&Request::Sample {
        offset: 0,
        limit: 3,
    });
    let responses = driver.run_default();
    assert!(matches!(
        &responses[1],
        Response::Error {
            code: ErrorCode::NoFile,
            ..
        }
    ));
}

#[test]
fn incomplete_stream_is_reported() {
    let mut driver = Driver::default();
    hello(&mut driver);
    driver.send(&Request::OpenStream { declared_size: 100 });
    driver.chunk(b"short");
    driver.send(&Request::Inspect);
    let responses = driver.run_default();
    assert!(matches!(
        &responses[1],
        Response::Error {
            code: ErrorCode::StreamIncomplete,
            ..
        }
    ));
}

#[test]
fn oversized_stream_is_rejected() {
    let mut driver = Driver::default();
    hello(&mut driver);
    driver.send(&Request::OpenStream {
        declared_size: 16 * 1024 * 1024 * 1024 + 1,
    });
    let responses = driver.run_default();
    assert!(matches!(
        &responses[1],
        Response::Error {
            code: ErrorCode::StreamTooLarge,
            ..
        }
    ));
}

#[test]
fn chunks_beyond_declared_size_are_fatal() {
    let mut driver = Driver::default();
    hello(&mut driver);
    driver.send(&Request::OpenStream { declared_size: 3 });
    driver.chunk(b"abcd");
    let responses = driver.run_default();
    assert!(matches!(
        &responses[1],
        Response::Error {
            code: ErrorCode::StreamTooLarge,
            ..
        }
    ));
    assert_eq!(responses.len(), 2);
}

#[test]
fn malformed_stream_reports_file_error() {
    let mut driver = Driver::default();
    hello(&mut driver);
    driver.send(&Request::OpenStream { declared_size: 12 });
    driver.chunk(b"not parquet!");
    driver.send(&Request::Inspect);
    let responses = driver.run_default();
    assert!(matches!(
        &responses[2],
        Response::Error {
            code: ErrorCode::FileError,
            ..
        }
    ));
}

#[test]
fn abort_discards_the_stream() {
    let mut driver = Driver::default();
    hello(&mut driver);
    driver.send(&Request::OpenStream { declared_size: 10 });
    driver.chunk(b"12345");
    driver.send(&Request::StreamAbort);
    driver.send(&Request::Inspect);
    let responses = driver.run_default();
    assert_eq!(responses[1], Response::StreamAborted);
    assert!(matches!(
        &responses[2],
        Response::Error {
            code: ErrorCode::NoFile,
            ..
        }
    ));
}

#[test]
fn settings_enable_optional_diagnostics_on_render() {
    let mut driver = Driver::default();
    hello(&mut driver);
    driver.stream_file(fixtures::CONSTANT_MARKET);
    driver.send(&Request::Inspect);
    driver.send(&Request::Render {
        options: PreviewOptions {
            sample_limit: 3,
            ..Default::default()
        },
    });
    let config = WorkerConfig {
        settings: Settings {
            market_profile: Some(market_profile()),
            baseline: None,
        },
        ..Default::default()
    };
    let responses = driver.run(config);
    match &responses[3] {
        Response::Preview { report, text } => {
            assert!(report.market.is_some());
            assert!(text.contains("Market diagnostics"));
            assert!(text.contains("Spread constancy"));
        }
        other => panic!("unexpected {other:?}"),
    }
}

#[test]
fn diagnose_without_profile_is_rejected() {
    let mut driver = Driver::default();
    hello(&mut driver);
    driver.stream_file(fixtures::TRADES);
    driver.send(&Request::Inspect);
    driver.send(&Request::Diagnose {
        market_profile: None,
        baseline: None,
        rows: None,
    });
    let responses = driver.run_default();
    assert!(matches!(
        &responses[3],
        Response::Error {
            code: ErrorCode::ProfileMissing,
            ..
        }
    ));
}

#[test]
fn scan_response_has_exact_count() {
    let mut driver = Driver::default();
    hello(&mut driver);
    driver.stream_file(fixtures::MULTI_ROW_GROUPS);
    driver.send(&Request::Inspect);
    driver.send(&Request::Scan);
    let responses = driver.run_default();
    match &responses[3] {
        Response::Scan { scan } => {
            assert_eq!(scan.exact_row_count, Some(100));
        }
        other => panic!("unexpected {other:?}"),
    }
}

#[test]
fn rejected_empty_stream_returns_to_idle_instead_of_leaving_a_spool() {
    let mut d = Driver::default();
    hello(&mut d);
    d.send(&Request::OpenStream { declared_size: 0 });
    d.send(&Request::Inspect);
    d.send(&Request::About);
    let r = d.run_default();
    assert!(matches!(
        r[1],
        Response::Error {
            code: ErrorCode::FileError,
            ..
        }
    ));
    assert!(matches!(
        r[2],
        Response::Error {
            code: ErrorCode::NoFile,
            ..
        }
    ));
    assert!(matches!(r[3], Response::About { .. }));
}
#[test]
fn failed_inspection_does_not_leave_a_consumed_spool_as_an_open_stream() {
    let mut d = Driver::default();
    hello(&mut d);
    d.send(&Request::OpenStream { declared_size: 8 });
    d.chunk(b"notafile");
    d.send(&Request::Inspect);
    d.send(&Request::Inspect);
    d.stream_file(fixtures::TRADES);
    d.send(&Request::Inspect);
    let r = d.run_default();
    assert!(matches!(
        r[2],
        Response::Error {
            code: ErrorCode::FileError,
            ..
        }
    ));
    assert!(matches!(
        r[3],
        Response::Error {
            code: ErrorCode::NoFile,
            ..
        }
    ));
    assert!(matches!(r.last().unwrap(), Response::Summary { .. }));
}

#[test]
fn empty_chunks_after_completion_do_not_duplicate_acceptance() {
    let mut driver = Driver::default();
    hello(&mut driver);
    driver.send(&Request::OpenStream { declared_size: 3 });
    driver.chunk(b"abc");
    for _ in 0..10_000 {
        driver.chunk(b"");
    }
    driver.send(&Request::About);
    driver.send(&Request::Shutdown);
    let responses = driver.run_default();
    assert_eq!(responses.len(), 4);
    assert!(matches!(responses[0], Response::Welcome { .. }));
    assert_eq!(responses[1], Response::StreamAccepted { bytes: 3 });
    assert!(matches!(responses[2], Response::About { .. }));
    assert_eq!(responses[3], Response::Bye);
}
