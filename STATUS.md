# Status

Made by Maria. Date: 2026-10-28.

## What is built

v1 scope from `docs/ARCHITECTURE.md`, end to end:

- **Read-only Parquet core** (`crates/core`): bounded seekable stream access,
  defensive footer parsing, schema and row-group summary, metadata-derived row
  count, top-level date/timestamp bounds with complete/partial/unavailable
  coverage, a virtualized sample with safe nested rendering, and an optional
  bounded full scan with cancellation and exact-count claims only when complete.
- **Optional market diagnostics**: explicit field mapping, constant-spread,
  open-equals-previous-close, return-tail excess kurtosis, zero returns, OHLC
  plausibility, time ordering, thresholds, sample coverage and warnings that are
  explicitly not proof or financial advice. A local dated baseline requires an
  explicit instrument and licence, and a missing/unlicensed baseline is shown as
  unavailable.
- **Versioned worker protocol** (`crates/protocol`) with a mandatory handshake,
  8 MiB control-frame cap, 256 KiB stream-chunk cap, bounded error messages and
  explicit stream completion/abort handling.
- **Isolated worker** (`crates/worker`) that receives streamed bytes only, writes
  them to a private bounded temporary spool, removes the spool on close and
  never opens a path supplied by the handler.
- **Worker client** (`crates/client`) for the handler and process tests, with
  version matching, stream transfer and hard process kill.
- **Windows COM preview handler** (`crates/windows`): per-user `.parquet`
  registration with backup/restore, `IInitializeWithStream`, `IPreviewHandler`,
  `IPreviewHandlerVisuals`, child preview window, F1 About screen, stream-only
  transfer, and a job-object-limited worker. Win32/COM is `#[cfg(windows)]` and
  type-checked on Linux with `--features win-check`.
- **Headless CLI** (`crates/cli`) for Linux/Windows diagnostics, text/JSON output
  and smoke tests. It is not the shipped standalone viewer.
- **Installer** (`installer/Register-ParquetPreview.ps1`) for opt-in per-user
  registration, a warning before changing associations, `-Force`/`-Yes` controls,
  and uninstall that restores the prior handler only when it is still safe.
- **Fixtures and golden tests** (`tests/fixtures`, `crates/fixtures`): multiple
  row groups, missing statistics, nested/nullable schema, all enabled codecs,
  malformed/encrypted/empty files, dated baseline and deterministic checksums.
- About, licence, protocol, testing, roadmap and third-party inventory docs.

The project performs no network access, uploads or telemetry. Input files are
opened read-only. No MT5 trading call or order path exists; MT5 is only mentioned
in read-only/trademark context and this project is not affiliated with
MetaQuotes.

## Build and test commands/results

Run on the Linux x86_64 implementation host with Rust 1.85.1 on 2026-10-28:

```text
$ cargo build --workspace
Finished `dev` profile [unoptimized + debuginfo] target(s)
exit 0

$ cargo test --workspace
21 suite result lines; 128 tests passed; 0 failed; 0 ignored
exit 0

$ cargo check -p parquet-preview-windows --features win-check
Finished `dev` profile [unoptimized + debuginfo] target(s)
exit 0

$ cargo run -p parquet-preview-cli -- --about
exit 0

$ cargo run -p parquet-preview-worker -- --version
parquet-preview-worker 0.1.0 protocol 1 (max control 8388608 bytes, max chunk 262144 bytes)

$ (cd tests/fixtures && sha256sum -c SHA256SUMS)
18 files: OK
```

The worker/client process test spawns the real worker binary and verifies stream
transfer, sampling, rendering, About text and clean shutdown. Golden snapshots
cover the rendered preview. Registration tests use an in-memory registry.

`cargo test --workspace` does **not** execute COM or Win32 code. See
`TESTING.md` for the manual Windows checklist.

## Lines of code

Measured with `find ... | xargs wc -l` (including blank/comment lines):

| Scope | Lines |
| --- | --- |
| Rust source (`crates/*/src`, excluding tests) | 10,099 |
| Rust tests (`crates/*/tests`) | 1,107 |
| Rust total | 11,206 |
| Docs/scripts (README, TESTING, ROADMAP, THIRD_PARTY, protocol, installer) | 657 |

The implementation is larger than the architecture's rough 1,200–2,200 LOC
guess, mostly because the parser, bounded renderer, protocol, golden fixtures and
Windows FFI surface are all included rather than stubbed.

## Tests requiring real Windows

1. COM activation and DLL loading in 64-bit Explorer.
2. Explorer's real `IStream` (`Stat` size, read behaviour, seek).
3. `regsvr32` registration, HKCU permissions, Explorer restart, conflicts.
4. Child window layout/resizing, focus, scrolling and the F1 About dialog.
5. Job-object assignment in a jobbed Explorer and worker limits.
6. Selection-change cancellation, worker kill and thread join without deadlock.
7. Blocked-file/MOTW, SmartScreen, antivirus and policy behaviour.
8. Backup/restore against real prior associations and foreign takeover.
9. 32-bit rejection/64-bit-only build.
10. MSIX/Store packaging (not implemented).

## Open issues

- **No real Windows verification.** The Win32/COM layer is type-checked only;
  several ABI/layout/runtime assumptions remain unproven until tested on Windows.
- **Not code-signed; no MSIX/Store package.** SmartScreen may warn; packaging and
  signing are roadmap items.
- **Job assignment can fail** when Explorer itself is in a job that disallows
  nested jobs. The handler currently reports an error rather than running
  without resource limits.
- **Page-level decompression is not independently capped.** Row-group compressed
  budgets, row limits and decoded-byte caps bound normal work, but the upstream
  reader can materialize one page before the core checks the decoded size. This
  is documented as residual risk.
- **Nested date/timestamp leaves** are excluded from the top-level bounds report;
  the limitation is shown in the UI.
- **Handler UI** requests one first sample page and does not offer a full-scan or
  page-navigation control; the core, worker and CLI support both scan and
  offset-based paging.
- **Blocking host stream read** can delay `Unload` because the preview thread is
  joined; the worker itself is killed first. Needs Windows stress testing.
- **Baseline data/licensing** is user-supplied local CSV only; the bundled
  fixture is synthetic. Heuristics can misclassify data.
- **Licence audit items:** `unicode-ident` (build-time proc-macro dependency,
  `(MIT OR Apache-2.0) AND Unicode-3.0`) is the only recorded package outside
  the brief's named licence list. It is not shipped in the binaries; target-only
  metadata also names wasm/r-efi/tiny-keccak packages that are not in the
  Windows/Linux build tree. No GPL/AGPL code is linked. See `THIRD_PARTY.md`.
- **Worker cancellation** is process termination, not a graceful cancel message;
  acceptable for v1 but worth revisiting for long scans.
- `uuid` is pinned to 1.18.1 in `Cargo.lock` to keep the Rust 1.85 MSRV; newer
  releases require 1.89.
