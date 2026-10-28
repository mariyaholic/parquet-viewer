Made by Maria

# Parquet viewer and preview

Preview a local Parquet file in Windows Explorer with schema, row-group
information and transparent scan coverage.

Free and open source (Apache-2.0). See [LICENSE](LICENSE) and [NOTICE](NOTICE).

Status: v1 implementation. Releases are not code-signed yet.

## v1 scope

- A per-user Windows Explorer preview handler for `.parquet` files.
- Footer schema, row-group list and metadata-derived row count. Counts and
  statistics are explicitly labelled until a complete scan verifies them.
- Date/timestamp bounds with `complete`, `partial` or `unavailable` coverage
  and a reason for each column.
- A virtualized, bounded sample with safe rendering of nested structs, lists,
  maps, decimals, dates, timestamps and binary values.
- An optional full decoded scan with row, decoded-byte, compressed-byte and
  time budgets. A cancelled or skipped scan never claims exactness.
- Optional market-data heuristics (constant spread, open equal to previous
  close, return tails, price plausibility) with an explicit field mapping,
  sample coverage and an optional local dated reference-price baseline.
- An About dialog (F1 in the preview pane) with "Made by Maria" and the
  "free and open source (Apache-2.0)" notice.
- A headless CLI for development, diagnostics and non-Windows verification.

There is no SQL, no standalone GUI viewer, no network access, no uploads and no
telemetry. MT5/MetaTrader support, if configured later, is strictly read-only:
this project places no orders and makes no trading calls. It is not affiliated
with MetaQuotes; MetaTrader 5 and MT5 are trademarks of MetaQuotes Ltd.

## Workspace

| Crate | Purpose |
| --- | --- |
| `crates/core` | Read-only Parquet inspection, summary, sample, scan, diagnostics, rendering |
| `crates/protocol` | Versioned, size-capped worker protocol |
| `crates/worker` | Isolated worker process and private stream spool |
| `crates/client` | Cross-platform worker process client |
| `crates/windows` | COM preview handler, registry registration, Win32 glue |
| `crates/cli` | Headless preview/diagnostic tool |
| `crates/fixtures` | Deterministic Parquet fixture generator |

See [docs/ARCHITECTURE.md](docs/ARCHITECTURE.md) for the product architecture,
[docs/PROTOCOL.md](docs/PROTOCOL.md) for the worker protocol and
[TESTING.md](TESTING.md) for verification.

## Build and test

Rust 1.85 or later and a normal Cargo toolchain:

```text
cargo build --workspace
cargo test --workspace
```

No Windows target is required for those commands. The Win32/COM module is
behind `#[cfg(windows)]`; on Linux it can be type-checked but not tested:

```text
cargo check -p parquet-preview-windows --features win-check
```

A real Windows build needs the MSVC Rust toolchain. See [TESTING.md](TESTING.md)
for everything that still requires real Windows.

## Headless use

```text
cargo run -p parquet-preview-cli -- tests/fixtures/trades.parquet --rows 20
cargo run -p parquet-preview-cli -- tests/fixtures/trades.parquet --scan
cargo run -p parquet-preview-cli -- tests/fixtures/trades.parquet --json
cargo run -p parquet-preview-cli -- --about
```

Market diagnostics are opt-in. Create a settings file and point the worker at it
with the `PARQUET_PREVIEW_SETTINGS` environment variable:

```json
{
  "market_profile": {
    "time_field": "time",
    "open_field": "open",
    "high_field": "high",
    "low_field": "low",
    "close_field": "close",
    "spread_field": "spread",
    "instrument": "EURUSD"
  },
  "baseline": {
    "path": "C:\\Users\\me\\baseline.csv",
    "instrument": "EURUSD",
    "licence": "internal research licence 2026"
  }
}
```

The baseline is local CSV with `date,price` plus an optional `instrument`
column, and it must name the licence/terms under which it is used. Without a
dated, instrument-specific, explicitly licensed baseline, the price comparison
is reported as unavailable. Heuristics are warnings, not proof of fraud,
authenticity or integrity, and are not financial advice.

## Windows registration

Build the workspace on Windows, then use the opt-in per-user script:

```text
powershell -ExecutionPolicy Bypass -File installer\Register-ParquetPreview.ps1
powershell -ExecutionPolicy Bypass -File installer\Register-ParquetPreview.ps1 -Unregister
```

The script warns before changing `.parquet` associations, backs up the previous
handler, and the uninstaller only restores it if the association is still ours.
Registration is under `HKCU\Software\Classes`; no elevation is required. Restart
Explorer after registering. Releases are not code-signed yet, so SmartScreen may
warn. See [installer/README.md](installer/README.md).

The handler and worker must be version-matched and live in the same directory:
`parquet_preview_windows.dll` and `parquet-preview-worker.exe`.

## Known limitations

- Encrypted Parquet files, corrupt files and unsupported logical types are
  reported rather than decoded. v1 handles the codecs enabled in the `parquet`
  crate: Snappy, Gzip, Zstd, Brotli and LZ4_RAW.
- Nested date/timestamp leaves are excluded from the top-level date-bounds
  report and the limitation is stated.
- Footer counts and statistics come from the file. They are not an integrity
  check; a full scan is required to claim exact counts.
- The preview handler targets 64-bit Windows. Store/MSIX packaging, signing,
  ARM64 and page-index-based virtual scrolling are not part of v1.
- The worker enforces budgets and is assigned to a job object, but a job object
  is not a security sandbox.

Made by Maria. Free and open source (Apache-2.0).
