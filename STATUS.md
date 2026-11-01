# Status

[measured] The local, read-only Parquet preview handler (v1) is built: a 64-bit
Windows COM preview-handler DLL with an isolated worker process, a versioned
host/worker protocol, footer/schema/row-group reading, a first-rows sample, an
optional full scan and an optional market-data red-flag profile.

[measured] This branch implements the Opus-review fix plan for this repository
(12 findings, one commit each) on top of the LF-normalization chore. Explorer
input is now marshalled to a background thread and streamed to the worker in
bounded 64 KiB chunks: there is no in-RAM file snapshot and no 64 MiB ceiling.
The handler registers per-user as an Apartment-threaded object, writes the
server definition before the `.parquet` association with best-effort rollback,
refreshes the association backup when a foreign handler took over, and resolves
the worker next to the loaded DLL in release builds (the debug-only
`PARQUET_PREVIEW_WORKER` override is compiled out of release binaries).

## Commits (one per finding)

| Finding | Commit | Summary |
|---|---|---|
| chore | `b299bda` | normalize line endings to LF (`.gitattributes`) |
| H1 | `ecedcdb` | emit CRLF to the Win32 preview edit control |
| M2 | `7dc395f` | delete spool temp files on worker kill |
| M3 | `4e08a16` | ship full third-party licence texts |
| L6 | `8b6d953` | remove the dead registry API, add real Windows coverage |
| L3 | `1c3428a` | drop the redundant PID kill path |
| L5 | `d66e5da` | host integration: `IObjectWithSite`/`IPreviewHandlerFrame`, font, colours |
| M1 | `26f5fc8` | marshalled, bounded stream transfer (removes the 64 MiB snapshot) |
| L4 | `bce549e` | register the handler as Apartment threaded |
| L7 | `a862df9` | docs corrected to describe current behaviour |
| L8 | `485728c` | debug-only worker override; release and `cargo deny` CI |
| L1 | `2c6daab` | refresh the association backup after a foreign takeover |
| L2 | `ceba042` | write the server definition before the association, with rollback |

## Build and test

Commands run locally (Rust 1.98.1; CI pins 1.85.1):

```text
cargo fmt --check                                  # exit 0
cargo clippy --locked --all-targets                # exit 0; only pre-existing lints
cargo build --locked --workspace                   # ok
cargo test --locked --workspace                    # 144 passed, 0 failed
PARQUET_PREVIEW_E2E=1 cargo test --locked -p parquet-preview-windows --lib \
  -- --test-threads=1 full_preview_round_trip      # 1 passed
cargo build --locked --release -p parquet-preview-windows   # ok
```

[measured] The release-profile DLL contains no `PARQUET_PREVIEW_WORKER` string
(`findstr` exits 1) while the debug DLL does (`findstr` exits 0); this proves the
L8 override is compiled out of shipped binaries.

[measured] Rust sources under `crates` total 11,899 lines across 39 files,
including tests.

## Tests that need real Windows

- The opt-in end-to-end COM harness (`crates/windows/src/com.rs`, run with
  `PARQUET_PREVIEW_E2E=1`) drives the real class factory, handler, `IStream` and
  isolated worker process together.
- The Windows-only registry test in `crates/windows/src/win32_registry.rs`.
- COM apartment behaviour, Explorer streams (including SMB and OneDrive
  placeholders), registration and uninstall conflicts, and cancellation must be
  verified in a real host; see `docs/TESTING.md`.
- The native release build `scripts/build-windows.ps1` requires a Windows run.

## Deviations from the fix-plan specs (also in each commit message)

- **M1:** `CancelReader` lives in the `client` crate (exported, platform-neutral,
  testable against the real worker) rather than a windows-local wrapper; a
  non-marshalable-stream fallback streams from the host apartment in the same
  bounded chunks; `copy_stream.rs` is deleted wholesale.
- **L5:** `SetSite` stores the `IPreviewHandlerFrame` queried out of the site (an
  owned reference released at refcount zero), not the site itself.
- **L7:** the roadmap's virtual-scrolling bullet was extended to cover pane
  paging rather than a new bullet added.
- **L8:** the release job adds `cargo fetch --locked` (the build script uses
  `--offline` on a fresh runner); new actions are unpinned with TODO comments,
  pending XR-1.
- **L2:** the failure-injecting test double keys on the target key and name, not
  on the Nth `set_string`, because the fix changes the call order and count.

## Open issues (for the Manager)

- **XR-1 (`chore/xr-1-ci-pins`, PR #2)** should merge and re-pin the GitHub
  Actions before the first tagged CI/release run. This branch edits
  `.github/workflows/ci.yml` as well (L8), so rebase after XR-1 lands.
- **L8 release format** (attach `dist/` from `scripts/build-windows.ps1` to a
  tag-triggered GitHub Release) is the spec's open question; shipped as
  described, left for confirmation.
- No Manager sign-off was required for any 07b finding beyond the C1 ruling and
  the recorded 07b M1 approval; every ⚠ sign-off is in
  `fix-plan/DECISIONS.md`.
- The preview report truncates the "Made by Maria" credit line before it prints
  (the e2e asserts on `Rows: 60` and CRLF instead). Observed, not a finding;
  worth a follow-up.
