// SPDX-License-Identifier: Apache-2.0
//! End-to-end process test: spawn the real worker binary and drive it.

use std::io::{Cursor, Read};
use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;

use parquet_preview_client::{CancelReader, WorkerClient};
use parquet_preview_core::PreviewOptions;
use parquet_preview_fixtures as fixtures;

/// Minimal Win32 process-wait surface, declared locally.
///
/// The worker test crate cannot reach `parquet-preview-windows::ffi` (that
/// module is private) and does not depend on the Windows crate, so the two
/// functions the kill assertion needs are declared here.
#[cfg(windows)]
mod win32 {
    #![allow(non_snake_case, clippy::upper_case_acronyms)]

    pub const SYNCHRONIZE: u32 = 0x0010_0000;
    pub const PROCESS_QUERY_LIMITED_INFORMATION: u32 = 0x1000;
    pub const WAIT_OBJECT_0: u32 = 0x0000_0000;
    pub type HANDLE = isize;

    #[link(name = "kernel32")]
    extern "system" {
        pub fn OpenProcess(access: u32, inherit: i32, pid: u32) -> HANDLE;
        pub fn WaitForSingleObject(handle: HANDLE, milliseconds: u32) -> u32;
        pub fn GetExitCodeProcess(handle: HANDLE, code: *mut u32) -> i32;
        pub fn CloseHandle(handle: HANDLE) -> i32;
    }
}

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
    #[cfg(windows)]
    let handle = unsafe {
        win32::OpenProcess(
            win32::SYNCHRONIZE | win32::PROCESS_QUERY_LIMITED_INFORMATION,
            0,
            pid,
        )
    };
    #[cfg(windows)]
    assert_ne!(handle, 0, "OpenProcess failed for pid {pid}");
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
    // On Windows the process object becomes signalled when the worker
    // terminates, so a bounded wait returns WAIT_OBJECT_0. `Child::kill`
    // terminates with exit code 1; a worker that merely saw its stdin close
    // would exit 0, so the exit code proves the kill actually ran. Previously
    // this platform asserted nothing at all.
    #[cfg(windows)]
    {
        let status = unsafe { win32::WaitForSingleObject(handle, 10_000) };
        let mut code = 0u32;
        let ok = unsafe { win32::GetExitCodeProcess(handle, &mut code) };
        unsafe { win32::CloseHandle(handle) };
        assert_eq!(
            status,
            win32::WAIT_OBJECT_0,
            "worker {pid} was still running after the client was dropped"
        );
        assert_ne!(ok, 0, "GetExitCodeProcess failed for pid {pid}");
        assert_eq!(
            code, 1,
            "worker {pid} exited with code {code}, not the kill code"
        );
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

/// A reader that produces a fixed number of bytes on the fly, so a large
/// transfer needs neither a fixture file nor a large allocation.
struct GeneratedBytes {
    remaining: u64,
    byte: u8,
}

impl Read for GeneratedBytes {
    fn read(&mut self, buffer: &mut [u8]) -> std::io::Result<usize> {
        let want = (buffer.len() as u64).min(self.remaining) as usize;
        for slot in &mut buffer[..want] {
            *slot = self.byte;
        }
        self.remaining -= want as u64;
        Ok(want)
    }
}

/// M1: the preview path no longer snapshots the file into RAM, so a stream far
/// larger than the old 64 MiB ceiling transfers in bounded chunks. 200 MiB is
/// produced on the fly and streamed to the worker; the worker accepts all of it.
#[test]
fn streams_a_two_hundred_mib_reader_without_a_snapshot() {
    let size = 200 * 1024 * 1024;
    let mut client = WorkerClient::spawn(&worker_binary(), "large-stream-test").unwrap();
    let cancel = AtomicBool::new(false);
    let mut source = GeneratedBytes {
        remaining: size,
        byte: 0x5a,
    };
    let mut reader = CancelReader::new(&mut source, &cancel);
    client.open_stream(&mut reader, size).unwrap();
    client.kill();
}

/// M1: the transfer stops between chunks once the preview is replaced, so a slow
/// `IStream` cannot pin the pane. Fails before the fix (the whole stream was
/// read into RAM first, so cancellation could not be observed mid-transfer).
#[test]
fn a_cancelled_transfer_stops_between_chunks() {
    struct CancelAfterTwoChunks {
        remaining: u64,
        cancel: Arc<AtomicBool>,
        reads: u32,
    }

    impl Read for CancelAfterTwoChunks {
        fn read(&mut self, buffer: &mut [u8]) -> std::io::Result<usize> {
            self.reads += 1;
            if self.reads > 2 {
                self.cancel.store(true, Ordering::Relaxed);
            }
            let want = (buffer.len() as u64).min(self.remaining) as usize;
            buffer[..want].fill(0x21);
            self.remaining -= want as u64;
            Ok(want)
        }
    }

    // Enough for many chunks, so cancellation lands mid-transfer.
    let size = 8 * 1024 * 1024;
    let mut client = WorkerClient::spawn(&worker_binary(), "cancel-stream-test").unwrap();
    let cancel = Arc::new(AtomicBool::new(false));
    let mut source = CancelAfterTwoChunks {
        remaining: size,
        cancel: cancel.clone(),
        reads: 0,
    };
    let mut reader = CancelReader::new(&mut source, &cancel);
    let result = client.open_stream(&mut reader, size);
    assert!(
        result.is_err(),
        "the transfer completed even though cancellation was requested"
    );
    client.kill();
}
