// SPDX-License-Identifier: Apache-2.0
//! End-to-end process test: spawn the real worker binary and drive it.

use std::io::Cursor;
use std::path::PathBuf;

use parquet_preview_client::WorkerClient;
use parquet_preview_core::PreviewOptions;
use parquet_preview_fixtures as fixtures;

fn worker_binary() -> PathBuf {
    PathBuf::from(env!("CARGO_BIN_EXE_parquet-preview-worker"))
}

fn fixture(name: &str) -> PathBuf {
    fixtures::fixture_dir(env!("CARGO_MANIFEST_DIR")).join(name)
}

#[test]
fn process_round_trip_and_clean_shutdown() {
    let mut client = WorkerClient::spawn(&worker_binary(), "process-test").unwrap();
    assert_eq!(
        client.welcome().protocol_version,
        parquet_preview_protocol::PROTOCOL_VERSION
    );
    let bytes = std::fs::read(fixture(fixtures::TRADES)).unwrap();
    client
        .open_stream(&mut Cursor::new(&bytes), bytes.len() as u64)
        .unwrap();
    let summary = client.inspect().unwrap();
    assert_eq!(summary.row_count.value, 60);
    let sample = client.sample(0, 4).unwrap();
    assert_eq!(sample.returned, 4);
    let (report, text) = client
        .render(PreviewOptions {
            sample_limit: 2,
            ..Default::default()
        })
        .unwrap();
    assert_eq!(report.summary.row_count.value, 60);
    assert!(text.contains("Made by Maria"));
    assert!(client.about().unwrap().contains("free and open source"));
    client.shutdown().unwrap();
}

#[test]
fn short_stream_is_rejected_without_hanging() {
    let mut client = WorkerClient::spawn(&worker_binary(), "process-test").unwrap();
    let result = client.open_stream(&mut Cursor::new(&b"abc"[..]), 100);
    assert!(result.is_err());
}

#[test]
fn duplicate_open_replaces_the_file() {
    let mut client = WorkerClient::spawn(&worker_binary(), "process-test").unwrap();
    let first = std::fs::read(fixture(fixtures::TRADES)).unwrap();
    client
        .open_stream(&mut Cursor::new(&first), first.len() as u64)
        .unwrap();
    assert_eq!(client.inspect().unwrap().row_count.value, 60);
    let second = std::fs::read(fixture(fixtures::MISSING_STATS)).unwrap();
    client
        .open_stream(&mut Cursor::new(&second), second.len() as u64)
        .unwrap();
    assert_eq!(client.inspect().unwrap().row_count.value, 30);
}

#[test]
fn dropping_client_kills_worker() {
    let client = WorkerClient::spawn(&worker_binary(), "process-test").unwrap();
    let pid = client.pid();
    drop(client);
    // The process should be gone; on Linux signalling 0 returns an error.
    #[cfg(target_os = "linux")]
    {
        let status = std::process::Command::new("kill")
            .arg("-0")
            .arg(pid.to_string())
            .status()
            .unwrap();
        assert!(!status.success());
    }
}

/// Spool temp files created by the worker `pid`, if any.
#[cfg(windows)]
fn spool_files_for(pid: u32) -> Vec<PathBuf> {
    let prefix = format!("parquet-preview-{pid}-");
    let Ok(entries) = std::fs::read_dir(std::env::temp_dir()) else {
        return Vec::new();
    };
    entries
        .flatten()
        .map(|entry| entry.path())
        .filter(|path| {
            path.file_name()
                .and_then(|name| name.to_str())
                .map(|name| name.starts_with(&prefix) && name.ends_with(".tmp"))
                .unwrap_or(false)
        })
        .collect()
}

/// A hard kill (`TerminateProcess`) runs no Rust destructors, so the spool file
/// must be removed by the OS (delete-on-close), not by `Drop`. Fails before the
/// M2 fix and passes after it.
#[cfg(windows)]
#[test]
fn killing_the_worker_leaves_no_spool_file() {
    let mut client = WorkerClient::spawn(&worker_binary(), "spool-kill-test").unwrap();
    let pid = client.pid();
    let bytes = std::fs::read(fixture(fixtures::TRADES)).unwrap();
    client
        .open_stream(&mut Cursor::new(&bytes), bytes.len() as u64)
        .unwrap();
    assert!(
        !spool_files_for(pid).is_empty(),
        "the live worker should hold a spool file before the kill"
    );
    client.kill();
    // Give the kernel a moment to release the terminated process's handles.
    let mut left = spool_files_for(pid);
    for _ in 0..40 {
        if left.is_empty() {
            break;
        }
        std::thread::sleep(std::time::Duration::from_millis(50));
        left = spool_files_for(pid);
    }
    assert!(left.is_empty(), "spool file survived the kill: {left:?}");
}
