// SPDX-License-Identifier: Apache-2.0
//! Child-only environment overrides; no real user settings or shared temp sweep.
use std::fs::{self, OpenOptions};
use std::io::Cursor;
use std::process::{Command, Stdio};
use parquet_preview_protocol::{decode_control, read_frame, FrameLimits, Response};

struct Fixture(std::path::PathBuf);
impl Fixture {
    fn new() -> Self {
        let name = format!("parquet-startup-test-{}-{}", std::process::id(),
            std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap().as_nanos());
        let path = std::env::temp_dir().join(name);
        fs::create_dir(&path).unwrap();
        Self(path)
    }
    fn worker(&self) -> Command {
        let mut command = Command::new(env!("CARGO_BIN_EXE_parquet-preview-worker"));
        command.env("PARQUET_PREVIEW_SETTINGS", self.0.join("settings.json"))
            .env("TMP", &self.0).env("TEMP", &self.0).env("TMPDIR", &self.0)
            .stdin(Stdio::null());
        command
    }
}
impl Drop for Fixture {
    fn drop(&mut self) { let _ = fs::remove_dir_all(&self.0); }
}

#[test]
fn startup_keeps_old_unrelated_matching_files() {
    let fixture = Fixture::new();
    let path = fixture.0.join("parquet-preview-unrelated.tmp");
    let file = OpenOptions::new().write(true).create_new(true).open(&path).unwrap();
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
    let result = fixture.worker().output().unwrap();
    assert!(!result.status.success());
    assert_eq!(fs::read(path).unwrap(), b"broken JSON");
    let mut output = Cursor::new(result.stdout);
    let frame = read_frame(&mut output, &FrameLimits::default()).unwrap().unwrap();
    match decode_control::<Response>(frame).unwrap() {
        Response::Error { message, .. } => assert!(message.contains("Settings could not be loaded")),
        other => panic!("unexpected default preview/handshake: {other:?}"),
    }
    assert!(read_frame(&mut output, &FrameLimits::default()).unwrap().is_none());
}
