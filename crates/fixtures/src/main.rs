// SPDX-License-Identifier: Apache-2.0
//! Generate the committed Parquet fixtures.

use std::path::PathBuf;
use std::process::ExitCode;

fn main() -> ExitCode {
    let mut args = std::env::args_os().skip(1);
    let Some(dir) = args.next() else {
        eprintln!("usage: parquet-preview-fixtures <output-directory>");
        return ExitCode::from(2);
    };
    if args.next().is_some() {
        eprintln!("usage: parquet-preview-fixtures <output-directory>");
        return ExitCode::from(2);
    }
    let dir = PathBuf::from(dir);
    match parquet_preview_fixtures::write_all(&dir) {
        Ok(()) => match parquet_preview_fixtures::write_baseline(&dir.join("baseline.csv")) {
            Ok(()) => {
                println!("wrote fixtures to {}", dir.display());
                ExitCode::SUCCESS
            }
            Err(err) => {
                eprintln!("failed to write baseline: {err}");
                ExitCode::FAILURE
            }
        },
        Err(err) => {
            eprintln!("failed to write fixtures: {err}");
            ExitCode::FAILURE
        }
    }
}
