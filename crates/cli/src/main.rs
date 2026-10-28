// SPDX-License-Identifier: Apache-2.0
//! Headless inspection tool used to verify the preview core on any platform.
//!
//! This is a developer/diagnostic companion, not the shipped Windows viewer.
//! It opens local files read-only and makes no network calls.

use std::path::PathBuf;
use std::process::ExitCode;

use parquet_preview_core::about::about_text;
use parquet_preview_core::baseline::BaselineSpec;
use parquet_preview_core::market::MarketProfile;
use parquet_preview_core::{Limits, PreviewOptions, Session, Settings};

const USAGE: &str = "\
parquet-preview-cli - headless Parquet preview (read-only)

Usage:
  parquet-preview-cli [options] <file>
  parquet-preview-cli --about

Options:
  --rows <n>         Sample rows to show (default 100, capped by the core)
  --offset <n>       Virtualized sample offset (default 0)
  --scan             Run the optional bounded full scan
  --diagnose         Enable optional market heuristics
  --profile <file>   Market profile JSON (otherwise from settings)
  --baseline <file>  Dated reference-price CSV
  --settings <file>  Settings JSON (otherwise the per-user default)
  --json             Print the structured report instead of text
  --about            Show About and licence information
  -h, --help         Show this help
  -V, --version      Show the version

All input is read-only. No network access, uploads or telemetry.
Made by Maria. Free and open source (Apache-2.0).
";

fn main() -> ExitCode {
    match run() {
        Ok(code) => code,
        Err(message) => {
            eprintln!("error: {message}");
            ExitCode::FAILURE
        }
    }
}

fn run() -> std::result::Result<ExitCode, String> {
    let mut args = std::env::args().skip(1);
    let mut file: Option<PathBuf> = None;
    let mut rows = 100usize;
    let mut offset = 0u64;
    let mut scan = false;
    let mut diagnose = false;
    let mut profile_path: Option<PathBuf> = None;
    let mut baseline_path: Option<PathBuf> = None;
    let mut settings_path: Option<PathBuf> = None;
    let mut json = false;

    while let Some(arg) = args.next() {
        match arg.as_str() {
            "-h" | "--help" => {
                print!("{USAGE}");
                return Ok(ExitCode::SUCCESS);
            }
            "-V" | "--version" => {
                println!("parquet-preview-cli {}", env!("CARGO_PKG_VERSION"));
                return Ok(ExitCode::SUCCESS);
            }
            "--about" => {
                println!("{}", about_text());
                return Ok(ExitCode::SUCCESS);
            }
            "--rows" => rows = parse_usize(args.next(), "--rows")?,
            "--offset" => offset = parse_u64(args.next(), "--offset")?,
            "--scan" => scan = true,
            "--diagnose" => diagnose = true,
            "--json" => json = true,
            "--profile" => profile_path = Some(PathBuf::from(required(args.next(), "--profile")?)),
            "--baseline" => {
                baseline_path = Some(PathBuf::from(required(args.next(), "--baseline")?))
            }
            "--settings" => {
                settings_path = Some(PathBuf::from(required(args.next(), "--settings")?))
            }
            other if other.starts_with('-') => {
                return Err(format!("unknown option {other}\n\n{USAGE}"));
            }
            other => {
                if file.is_some() {
                    return Err("only one file may be inspected".to_string());
                }
                file = Some(PathBuf::from(other));
            }
        }
    }
    let Some(file) = file else {
        return Err(format!("missing file\n\n{USAGE}"));
    };

    let settings = match &settings_path {
        Some(path) => Settings::load_from(path).map_err(|e| e.to_string())?,
        None => Settings::load_default().map_err(|e| e.to_string())?,
    };
    let mut options = PreviewOptions {
        display_name: file
            .file_name()
            .map(|name| name.to_string_lossy().to_string()),
        sample_offset: offset,
        sample_limit: rows,
        scan,
        market_profile: settings.market_profile.clone(),
        baseline: settings.baseline.clone(),
        diagnostic_rows: 50_000,
    };
    if let Some(path) = &profile_path {
        let bytes = std::fs::read(path)
            .map_err(|e| format!("cannot read profile {}: {e}", path.display()))?;
        let profile: MarketProfile = serde_json::from_slice(&bytes)
            .map_err(|e| format!("invalid profile JSON: {e}"))?;
        profile.validate().map_err(|e| e.to_string())?;
        options.market_profile = Some(profile);
    }
    if let Some(path) = &baseline_path {
        options.baseline = Some(BaselineSpec {
            path: path.to_string_lossy().to_string(),
            instrument: options
                .market_profile
                .as_ref()
                .and_then(|p| p.instrument.clone()),
        });
    }
    if diagnose {
        if options.market_profile.is_none() {
            return Err(
                "diagnostics need a market profile; pass --profile or configure settings"
                    .to_string(),
            );
        }
    } else {
        options.market_profile = None;
        options.baseline = None;
    }

    let session = Session::open_path(&file, &Limits::default()).map_err(|e| e.to_string())?;
    let report = session.preview(&options).map_err(|e| e.to_string())?;
    if json {
        println!(
            "{}",
            serde_json::to_string_pretty(&report)
                .map_err(|e| format!("cannot encode JSON: {e}"))?
        );
    } else {
        print!("{}", parquet_preview_core::report::render(&report));
    }
    Ok(ExitCode::SUCCESS)
}

fn required(value: Option<String>, name: &str) -> std::result::Result<String, String> {
    value.ok_or_else(|| format!("{name} needs a value"))
}

fn parse_usize(value: Option<String>, name: &str) -> std::result::Result<usize, String> {
    required(value, name)?
        .parse::<usize>()
        .map_err(|_| format!("{name} needs a non-negative integer"))
}

fn parse_u64(value: Option<String>, name: &str) -> std::result::Result<u64, String> {
    required(value, name)?
        .parse::<u64>()
        .map_err(|_| format!("{name} needs a non-negative integer"))
}
