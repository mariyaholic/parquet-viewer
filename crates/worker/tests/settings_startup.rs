// SPDX-License-Identifier: Apache-2.0
//! Child-only environment overrides; no real user settings or shared temp sweep.
use parquet_preview_protocol::{decode_control, read_frame, FrameLimits, Response};
use std::fs::{self, OpenOptions};
use std::io::Cursor;
use std::process::{Command, Stdio};

struct Fixture(std::path::PathBuf);

// The system clock can return the same value to concurrent tests. Create
// atomically and retry collisions without touching directories we do not own.
fn create_fixture_dir(
    root: &std::path::Path,
    process_id: u32,
    next: &std::sync::atomic::AtomicU64,
) -> std::path::PathBuf {
    use std::sync::atomic::Ordering;
    loop {
        let id = next.fetch_add(1, Ordering::Relaxed);
        let path = root.join(format!("parquet-startup-test-{process_id}-{id}"));
        match fs::create_dir(&path) {
            Ok(()) => return path,
            Err(error) if error.kind() == std::io::ErrorKind::AlreadyExists => continue,
            Err(error) => panic!(
                "could not create fixture directory {}: {error}",
                path.display()
            ),
        }
    }
}

impl Fixture {
    fn new() -> Self {
        static NEXT: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);
        Self(create_fixture_dir(
            &std::env::temp_dir(),
            std::process::id(),
            &NEXT,
        ))
    }
    fn worker(&self) -> Command {
        let mut command = Command::new(env!("CARGO_BIN_EXE_parquet-preview-worker"));
        command
            .env("PARQUET_PREVIEW_SETTINGS", self.0.join("settings.json"))
            .env("TMP", &self.0)
            .env("TEMP", &self.0)
            .env("TMPDIR", &self.0)
            .stdin(Stdio::null());
        command
    }
}
impl Drop for Fixture {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.0);
    }
}

#[test]
fn fixture_directory_retries_existing_name_without_removing_it() {
    let root = Fixture::new();
    let existing = root.0.join("parquet-startup-test-123-0");
    fs::create_dir(&existing).unwrap();
    fs::write(existing.join("owner"), b"keep").unwrap();
    let next = std::sync::atomic::AtomicU64::new(0);
    let created = create_fixture_dir(&root.0, 123, &next);
    assert_eq!(created, root.0.join("parquet-startup-test-123-1"));
    assert_eq!(fs::read(existing.join("owner")).unwrap(), b"keep");
}

#[test]
fn concurrent_fixture_directories_are_distinct() {
    let root = Fixture::new();
    let next = std::sync::atomic::AtomicU64::new(0);
    std::thread::scope(|scope| {
        let handles: Vec<_> = (0..32)
            .map(|_| scope.spawn(|| create_fixture_dir(&root.0, 123, &next)))
            .collect();
        let paths: std::collections::HashSet<_> = handles
            .into_iter()
            .map(|handle| handle.join().unwrap())
            .collect();
        assert_eq!(paths.len(), 32);
        assert!(paths.iter().all(|path| path.is_dir()));
    });
}

#[test]
fn startup_keeps_old_unrelated_matching_files() {
    let fixture = Fixture::new();
    let path = fixture.0.join("parquet-preview-unrelated.tmp");
    let file = OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(&path)
        .unwrap();
    file.set_modified(std::time::UNIX_EPOCH).unwrap();
    let result = fixture.worker().output().unwrap();
    assert!(result.status.success(), "{:?}", result.stderr);
    assert!(path.exists(), "startup removed a file it did not own");
    drop(file);
}

#[test]
fn invalid_settings_produce_visible_error_and_no_default_preview() {
    let fixture = Fixture::new();
    let path = fixture.0.join("settings.json");
    fs::write(&path, b"broken JSON").unwrap();
    let mut command = fixture.worker();
    command
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());
    let mut child = command.spawn().unwrap();
    parquet_preview_protocol::write_control(
        &mut child.stdin.take().unwrap(),
        &parquet_preview_protocol::Request::Hello {
            protocol_version: parquet_preview_protocol::PROTOCOL_VERSION,
            client: None,
        },
    )
    .unwrap();
    let result = child.wait_with_output().unwrap();
    assert!(!result.status.success());
    assert_eq!(fs::read(path).unwrap(), b"broken JSON");
    let mut output = Cursor::new(result.stdout);
    let frame = read_frame(&mut output, &FrameLimits::default())
        .unwrap()
        .unwrap();
    match decode_control::<Response>(frame).unwrap() {
        Response::Error { message, .. } => {
            assert!(message.contains("Settings could not be loaded"))
        }
        other => panic!("unexpected default preview/handshake: {other:?}"),
    }
    assert!(read_frame(&mut output, &FrameLimits::default())
        .unwrap()
        .is_none());
}

#[test]
fn actual_client_receives_settings_error() {
    let fixture = Fixture::new();
    let path = fixture.0.join("settings.json");
    fs::write(&path, b"broken JSON").unwrap();
    let result = Command::new(std::env::current_exe().unwrap())
        .args([
            "--ignored",
            "--exact",
            "settings_client_child",
            "--nocapture",
        ])
        .env("PARQUET_PREVIEW_SETTINGS", path)
        .env("PARQUET_SETTINGS_TEST_CHILD", "1")
        .output()
        .unwrap();
    assert!(
        result.status.success(),
        "stdout={} stderr={}",
        String::from_utf8_lossy(&result.stdout),
        String::from_utf8_lossy(&result.stderr)
    );
}

#[test]
#[ignore = "helper invoked only in an isolated child environment"]
fn settings_client_child() {
    assert_eq!(
        std::env::var("PARQUET_SETTINGS_TEST_CHILD").as_deref(),
        Ok("1")
    );
    match parquet_preview_client::WorkerClient::spawn(
        std::path::Path::new(env!("CARGO_BIN_EXE_parquet-preview-worker")),
        "settings-test",
    ) {
        Err(parquet_preview_client::ClientError::Worker {
            code: parquet_preview_protocol::ErrorCode::FileError,
            message,
        }) => assert!(message.contains("Settings could not be loaded")),
        Err(other) => panic!("lost actionable settings message: {other}"),
        Ok(_) => panic!("invalid settings unexpectedly produced a default preview"),
    }
}
