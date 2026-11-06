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
powershell -ExecutionPolicy Bypass -File installer\install.ps1 -DllPath .\parquet_preview_windows.dll
```

Then select a `.parquet` fixture in Explorer's preview pane, resize the pane,
switch selection while a preview is loading, and press F1. Pane paging through
a large file is not a v1 feature (see ROADMAP.md). Unregister and confirm the
prior association is restored. Test corrupt,
encrypted, empty, nested, multi-row-group and all-codec fixtures. Check
Task Manager for orphaned `parquet-preview-worker.exe` processes after switching
files repeatedly.

## Known test limitations

- Windows cross-builds do not execute COM/Win32 behavior.
- Timing tests (`max_scan_millis = 0`) are conservative and may complete on very
  small files; they only assert that an incomplete scan never claims exactness.
- Golden output includes the pinned Arrow/Parquet version from the committed
  fixture metadata.

## Cross-compilation and release artifacts

With the Windows GNU Rust target and MinGW already installed:

```text
CARGO_NET_OFFLINE=true cargo clippy --offline --all-targets --target x86_64-pc-windows-gnu
CARGO_NET_OFFLINE=true cargo test --offline --no-run --target x86_64-pc-windows-gnu
scripts/build-windows.sh
```

Cross-compiling test executables does not run them. The registration cleanup,
COM apartment behavior and Explorer integration still require Windows runtime
verification; the native release script also requires a Windows run.

## Round-two checks and limitations

Explorer stream input is marshalled to a background thread (`CoMarshalInterThreadInterfaceInStream` / `CoGetInterfaceAndReleaseStream`) and streamed to the worker in bounded 64 KiB chunks; the file is never copied into RAM and there is no size ceiling beyond the worker's own limit. The transfer checks the cancel flag between chunks, so replacing the selection interrupts a slow read. A stream that cannot be marshalled is streamed from the host apartment in the same bounded chunks (bounded memory, but the apartment is busy for the duration). Verify responsiveness, failures, repeated previews and cancellation with real Explorer streams, SMB paths and OneDrive placeholders on Windows.

Uninstall leaves shared association keys in place after removing the owned value, avoiding a non-atomic empty-key deletion. Verify real registry behaviour and conflict restoration on Windows, including concurrent changes. The ownership check and value removal themselves are separate registry operations, so concurrent reassignment remains a race; avoid simultaneous association changes during uninstall.

Run `scripts/build-windows.ps1` on Windows and compare independent release builds. Cross-builds do not establish native reproducibility or execute COM. The release payload must retain LICENSE, NOTICE, THIRD_PARTY.md and THIRD_PARTY_LICENSES.txt, the per-user registration scripts, and the single `parquet-preview-<version>-x64.zip` archive.

## Hosted acceptance follow-up — 2026-11-02 (Linux + Wine 11)

Wine execution is not native Explorer acceptance. Commands and raw output are
in the portfolio's `verification/` directory. The opt-in COM e2e harness is
explicitly enabled, not counted from its default early return.

### XR-2 item-by-item
1. Rendering: **partial**. Golden rendering and explicitly enabled real COM
   round-trip pass under Wine; native Explorer schema/sample layout is pending.
2. Error text: **partial**. Invalid-file and CRLF regressions run; native
   Explorer invalid-file presentation is pending.
3. Streaming: **partial**. Real COM stream marshalling and the 200 MiB client
   transfer test are included without skip filters. Native SMB/selection-change
   responsiveness remains pending; see STATUS.md for final suite outcome.
4. Temp leak: **partial**. Delete-on-close worker-kill is covered; unsafe shared-temp sweeping
   is disabled and unrelated/active-file preservation has a regression; native x20 Explorer changes and job-limit stress are pending.
5. Takeover backup: **automated corroboration passed**. Foreign takeover,
   force-register and restore regressions run. Native installer workflow is pending.
6. Registration order: **automated corroboration passed** via a write-failure
   injection proving association unchanged. Optional live interruption is pending.
7. No stray kill: **partial**. Redundant PID-kill path was removed previously;
   real worker-kill/drop tests run. Native 30-second arrow stress is pending.
8. Apartment: **partial**. Cross-apartment stream test passes with the source
   STA pumping COM messages. Native prevhost STA and re-registration remain pending.
9. Site/visuals: **native blocked**. Interface tests and COM e2e corroborate
   integration, not Explorer Tab traversal, dark mode or physical 150% DPI.
10. E2e harness: **Wine passed with PARQUET_PREVIEW_E2E=1**. It drives the
    real class factory, IStream, child window and isolated worker. Native run pending.
11. M4 Low-IL access denial: **blocked, not fixed speculatively**. The worker
    creates its spool using std::env::temp_dir and OpenOptions read/write/delete
    access; an I/O failure is returned as Internal during open_stream. This
    supports the temp-permissions hypothesis, not a proven native root cause.
    Repeat the clean native Explorer DWORD experiment from the root M4 spec,
    observe a rendered sample and prevhost integrity, then obtain a Manager
    variant choice before changing isolation or spool location. Wine cannot
    supply that decisive integrity-policy evidence. No DWORD/LocalLow change made.

The two-apartment test no longer blocks its STA in join(): it pumps messages
until the reader finishes, asserts the original complete payload, and has a
20-second failure deadline. No marshalling tests are skipped.
