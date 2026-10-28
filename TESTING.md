# Testing

Made by Maria.

This file records verification and the checks that require real Windows. All
date-stamped results are from the implementation host (Linux x86_64, Rust
1.85.1) unless a row says Windows.

## Commands and expected results

```text
cargo build --workspace
cargo test --workspace
cargo check -p parquet-preview-windows --features win-check
```

The Linux `win-check` command type-checks the raw Win32/COM surface. It does not
link Windows libraries and does not prove runtime behaviour.

## Automated coverage

| Area | Tests |
| --- | --- |
| Footer parser | short, wrong magic, truncated, oversized footer, encrypted magic |
| Summary | schema tree, nested groups, row-group/schema caps, row counts |
| Date bounds | complete, missing statistics, scan-verified bounds, unadjusted timestamps |
| Sample | pagination, offsets, nulls, nested struct/list, decimal/binary/date, byte budgets |
| Codecs | Snappy, Gzip, Zstd, Brotli, LZ4_RAW fixtures |
| Scan | complete, row budget, compressed budget, cancellation, empty file |
| Market diagnostics | constant spread, open/previous close, zero returns, invalid OHLC, baseline, missing/invalid mappings, cancellation |
| Rendering | golden snapshots, truncation, escaping |
| Settings | round-trip, unknown fields, invalid profile |
| Protocol | frame round-trip, caps, truncation, unknown tag, request/response |
| Worker | handshake, stream completion/abort/oversize/incomplete, malformed file, sample/scan/render, settings opt-in |
| Client | real worker process, short stream, replacement, drop kills child |
| Registration | fresh install, conflict, forced backup/restore, foreign-handler preservation, idempotence |

Fixtures are generated deterministically by `parquet-preview-fixtures` and
committed under `tests/fixtures`. Golden text is under
`crates/core/tests/golden`. Regenerate only when the fixture or rendered format
intentionally changes:

```text
cargo run -p parquet-preview-fixtures -- tests/fixtures
UPDATE_GOLDEN=1 cargo test -p parquet-preview-core --test golden
```

## Requires real Windows

These behaviours cannot be verified on this Linux host:

1. Loading the DLL into a 64-bit Explorer process and COM activation.
2. `IInitializeWithStream` with Explorer's actual stream implementation and
   `Stat` size behaviour.
3. Preview-handler registration via `regsvr32`, per-user hive permissions,
   Explorer restart and handler conflicts.
4. Child-window layout, resizing, keyboard focus, F1 About dialog and scrolling.
5. Job-object assignment in an Explorer process that may itself be in a job;
   nested job semantics and memory/process limits.
6. Worker cancellation by selection change, killing the job and joining the
   preview thread without deadlock or orphaned processes.
7. Blocked-file, MOTW, antivirus/SmartScreen and policy behaviour.
8. Registration backup/restore against real prior associations, including a
   product taking over `.parquet` after installation.
9. 32-bit host rejection and a 64-bit-only build (the crate intentionally
   refuses 32-bit Windows at compile time).
10. MSIX/Store packaging (not implemented in v1).

## Manual Windows checklist

```text
cargo build --release -p parquet-preview-worker -p parquet-preview-windows
# copy the DLL and worker into one directory
powershell -ExecutionPolicy Bypass -File installer\Register-ParquetPreview.ps1 -DllPath .\parquet_preview_windows.dll
```

Then select a `.parquet` fixture in Explorer's preview pane, resize the pane,
page through a large file, switch selection while a preview is loading, and
press F1. Unregister and confirm the prior association is restored. Test corrupt,
encrypted, empty, nested, multi-row-group and all-codec fixtures. Check
Task Manager for orphaned `parquet-preview-worker.exe` processes after switching
files repeatedly.

## Known test limitations

- The Windows COM/Win32 code is type-checked, not executed, on Linux.
- Timing tests (`max_scan_millis = 0`) are conservative and may complete on very
  small files; they only assert that an incomplete scan never claims exactness.
- Golden output includes the pinned Arrow/Parquet version from the committed
  fixture metadata.
