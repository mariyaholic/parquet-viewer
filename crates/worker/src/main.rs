// SPDX-License-Identifier: Apache-2.0
//! Isolated worker process entry point.

use std::io;
use std::process::ExitCode;

use parquet_preview_core::Settings;
use parquet_preview_worker::{serve, version_line, WorkerConfig};

fn main() -> ExitCode {
    let args = std::env::args().skip(1).collect::<Vec<_>>();
    if args.iter().any(|arg| arg == "--version" || arg == "-V") {
        println!("{}", version_line());
        return ExitCode::SUCCESS;
    }
    if args.iter().any(|arg| arg == "--help" || arg == "-h") {
        println!("{}", version_line());
        println!("Reads the versioned preview protocol on stdin and writes it on stdout.");
        println!("Read-only. No network access, uploads or telemetry. Made by Maria.");
        return ExitCode::SUCCESS;
    }

    let settings = match Settings::load_default() {
        Ok(settings) => settings,
        Err(error) => {
            eprintln!("parquet-preview-worker: settings ignored: {error}");
            Settings::default()
        }
    };
    let config = WorkerConfig {
        settings,
        ..Default::default()
    };
    let stdin = io::stdin();
    let stdout = io::stdout();
    match serve(stdin.lock(), stdout.lock(), config) {
        Ok(()) => ExitCode::SUCCESS,
        Err(error) => {
            eprintln!("parquet-preview-worker: {error}");
            ExitCode::FAILURE
        }
    }
}
