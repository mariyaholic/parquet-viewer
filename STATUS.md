# Status

## Standalone desktop architecture — proposed 2026-11-02

- Added `docs/STANDALONE-ARCHITECTURE.md`: GUI boundaries, truthful sample/scan
  capabilities, worker lifecycle, theme contract, staged delivery and acceptance.
- Grounded in both engines, trading desk/mic-mute native UI and pinned MH Ticker
  source plus Paper/PC-98 screenshots. No runtime dependencies or application code changed.
- Documentation/source-reference checks passed; no GUI, benchmark, native accessibility
  validation or new application test pass is claimed. Start implementation at S0.

## Latest hosted verification — 2026-11-02

[measured] Current tracked RS source: 12,846 lines across 39 files,
including tests (raw physical lines, not a production-only LOC estimate).

This section supersedes historical local/CI assumptions below. Changes are on
`fix/remaining-plan-issues`; no push, CI trigger or release publication occurred.

- **Marshalling-test deadlock fixed:** the source STA pumps COM messages until
  the worker finishes instead of blocking in join(). Full payload assertions
  remain and a 20 s deadline fails a real hang. No skip filter is used.
- **M4 investigated, deliberately not implemented:** the worker creates its
  spool in std::env::temp_dir with read/write/delete access and reports I/O
  failure as Internal during open_stream. The Low-IL/temp-permission hypothesis
  is plausible, not confirmed by Wine. The spec still requires a clean native
  Explorer flag experiment and an explicit Manager choice of opt-out versus
  Low-IL-preserving spool location. No integrity boundary was changed.

Verified with Rust/Cargo 1.99.1, MinGW GCC 12, Wine 11, serial test execution:
- `cargo fmt --all -- --check`: pass.
- `cargo clippy --workspace --all-targets --locked --offline`: pass on Linux
  and with `--target x86_64-pc-windows-gnu`; existing non-strict warnings remain.
- Linux workspace build/test: **135 passed, 0 failed**.
- Windows GNU workspace build: pass. Full unfiltered workspace tests with
  `PARQUET_PREVIEW_E2E=1` and runner `wine`: **144 passed, 0 failed**, including
  real COM e2e, cross-thread marshalling and the 200 MiB streaming client test.
- An initial link failed with No space left on device. The next run reached
  the large transfer but failed with BrokenPipe while only 154 MiB remained.
  After removing generated Linux executables/caches, the entire unfiltered
  Windows suite passed; both earlier failure logs are retained, not overwritten.
- `cargo deny --offline check licenses bans`: pass (allowance warnings).
- `sh scripts/build-windows.sh` with VERSION=0.1.0-local: real GNU release
  build passed. The zip has **12 entries** (the historical 13-entry claim below
  was stale); each entry matches dist, and all 11 manifest-listed files hash
  correctly. The release DLL contains no PARQUET_PREVIEW_WORKER string.
- Release CLI rendered trades.parquet under Wine.

Evidence: portfolio `verification/parquet-windows-acceptance.log`,
`FINAL-RESULTS.json`, release-build/smoke logs and `release-payloads.json`.
The committed PowerShell licence-generator -Check, native MSVC/PowerShell
package build, pinned Rust 1.85.1 CI run, Explorer Low-IL/DPI/SMB behavior and
physical desktop stress remain native/hosted checks. `docs/TESTING.md` lists
XR-2 outcomes and M4 prerequisites explicitly.


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
`PARQUET_PREVIEW_WORKER` override is compiled out of release binaries). A
follow-up batch pins the CI actions (XR-1), fixes the e2e text read, and
delivers releases as a single zip with conventional install entry points.

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
  -- --test-threads=1 full_preview_round_trip      # 1 passed (asserts the credit line)
cargo build --locked --release -p parquet-preview-windows   # ok
pwsh scripts/generate-third-party-licenses.ps1 -Check        # up to date (both powershell 5.1 and pwsh 7)
pwsh scripts/build-windows.ps1 -Version v0.1.0-test          # wrote dist/ and the release zip
```

`cargo deny check licenses bans` runs in CI (the tool is not installed on this
box); it is unchanged by this branch.

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

## Follow-ups after the fix batch (branch `chore/opus-review-07b-followups`)

| Change | Commit | Summary |
|---|---|---|
| XR-1 | `ed5ccbc` | pin all four CI actions by commit SHA; keep `AGENTS.md`/`opencode.json` local-only (the same commit that updates PR #2, rebased onto `main`) |
| e2e | `01eb077` | size the preview-text read from the control; assert the credit line |
| L8 | `e821de1` | deliver one `parquet-preview-<version>-x64.zip`; conventional `install.ps1`/`uninstall.ps1` entry points |
| licences | `1e2ba16` | sort licence files ordinally so the bundle `-Check` is engine-independent |

[measured] `scripts/build-windows.ps1 -Version v0.1.0-test` writes `dist/` with
the three binaries, four licence documents, the three registration scripts,
`INSTALL.md`, `SHA256SUMS`, and `parquet-preview-v0.1.0-test-x64.zip` (13 entries
inside the zip). The release-profile DLL contains no `PARQUET_PREVIEW_WORKER`
string (`findstr` exit 1) while the debug DLL does (`findstr` exit 0).

[measured] With the fixed 8,192-element read restored, the new e2e credit
assertion panics (red at `crates/windows/src/com.rs`); with the control-sized
buffer the opt-in harness passes (1 passed).

[measured] `generate-third-party-licenses.ps1 -Check` was red under PowerShell 7
on `main` (the committed bundle held the Windows PowerShell 5.1 collation order,
because `Sort-Object` orders `LICENSE.txt` before `LICENSE-MIT` on 5.1 and after
it on 7). After the ordinal sort it passes under both engines, and the bundle
change is a pure reorder (identical line multiset).

## Open issues (for the Manager)

- **XR-1 (PR #2)** is rebased on `main` (`40f6011`) and pins all four actions
  (checkout, rust-toolchain, cargo-deny-action, action-gh-release). This branch
  is built on it, so when PR #2 merges this PR's diff drops the XR-1 commit.
- **L8 release format (decided):** delivery is one zip; the installer follows the
  portable-zip plus per-user-script convention (a versioned MSI/MSIX stays on
  ROADMAP). Flagged in the commit and the PR body.
- No Manager sign-off was required for any 07b finding beyond the C1 ruling and
  the recorded 07b M1 approval; every ⚠ sign-off is in `fix-plan/DECISIONS.md`.
