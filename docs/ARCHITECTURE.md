# Parquet viewer and preview

## Standalone desktop companion — implemented

The portable Windows desktop companion now uses a shared native shell and
worker-backed format adapter without requiring COM registration. See the
[user guide](USER-GUIDE.md), [standalone architecture](STANDALONE-ARCHITECTURE.md)
and [current status](../STATUS.md) for delivered behavior and remaining acceptance.

The original Explorer/CLI research and proposed scope below are retained as
historical design notes, not a current feature inventory. Their machine-local
evidence links are not portable download links. The later desktop companion
extends that original scope; Explorer security gates remain unchanged.

## Six fields

1. **One-line pitch:** [inferred] Preview a local Parquet file in Explorer with schema, row-group information and transparent scan coverage.
2. **Who it's for:** [inferred] Analysts and developers quickly inspecting unfamiliar Parquet files without first writing a query; a trading profile is optional.
3. **5-second demo shot:** [guess] Select a Parquet file and see schema, footer row count, first rows and whether date bounds are complete.
4. **Overlap check:** [inferred] DuckDB, Tad and existing Parquet viewers already inspect tables; some offer querying and statistics. Windows Explorer, PowerToys Peek and installed preview handlers may show file previews depending on configuration. Verify current capabilities and compare on the same files before claiming a distinction.
5. **Reuse:** [measured] Ōsōji's [scan result](/workspace/desk/ava/repo-survey/targets/osoji/internal/scan/scan.go:19) includes skip reasons; borrow its reviewable-result concept after rights review. [inferred] The reader, preview handler and diagnostics are new; no existing Parquet reader is established in the supplied architecture ([ARCHITECTURE_v4.md](/workspace/qol-portfolio/ARCHITECTURE_v4.md:146)).
6. **Caveats:** [inferred] Explorer integration is Windows-specific; encrypted, corrupt or unsupported files and nested values may limit inspection. Footer counts and statistics are file-provided metadata, not proof of data integrity. Optional trading heuristics are warnings, not proof of fraud or authenticity; not financial advice. GitHub and winget binaries are “not code-signed yet” unless signing is approved; SmartScreen may warn.

## Edge over defaults

[measured] FLUORITE-EYES had no Parquet preview handler; DuckDB returned schema, row-group statistics and red-flag SQL in under one second on 1.4 million rows, while parquet-tools showed metadata and sample rows ([COMPARISON.md](/workspace/qol-portfolio/COMPARISON.md:29)). [documented] ParquetViewer and Fast Parquet Viewer are standalone tools; no open-source Explorer preview handler was found in that search, which does not prove none exists. Tad and Windows-only GUIs were not run ([comparison](/workspace/qol-portfolio/COMPARISON.md:35), [limitations](/workspace/qol-portfolio/COMPARISON.md:66)).

[inferred] **Build with narrowed edge:** a read-only Explorer preview handler showing schema, footer row count, row-group stats and first rows; no SQL and no standalone viewer. An optional market-data red-flag profile reports sample and coverage. [measured] On two synthetic fixtures open-equals-previous-close was 0.7394 versus 1.0, spread counts 79 versus 1, excess kurtosis 78.2 versus −0.01, and Friday close tracked US DST versus fixed UTC ([COMPARISON.md](/workspace/qol-portfolio/COMPARISON.md:39)). [inferred] Real-data thresholds are unknown; these are heuristics, not proof.

## Architecture

- **Stack:** [inferred] Rust native COM preview-handler DLL and isolated worker executable; native COM interop and mature Parquet library options justify the deviation from Go. Select and audit a maintained reader and its compression dependencies before implementation.
- **Modules:** [inferred] About screen showing “Made by Maria”; minimal handler, versioned worker protocol, bounded stream transfer, footer/schema and row-group reader, first-rows sample, optional full scan and optional market diagnostics. (Virtualized sample paging and scrolling are v2; see ROADMAP.md.) Display nested values safely; cap rows, decompressed bytes, memory and time, and cancel on selection changes. A footer row count is labelled metadata-derived; a full decoded count is exact only after a complete scan. Date bounds are complete only if verified for every relevant row group with trustworthy compatible statistics or by a complete scan; otherwise label partial/unavailable and identify coverage.
- **Key Windows APIs:** [inferred] `IInitializeWithStream`, `IPreviewHandler`, COM registration, child window and process/job controls. Transfer the supplied seekable stream via bounded IPC or private bounded temporary spool, not an assumed path. Enforce worker limits; a job object alone is not a security sandbox. Validate host bitness and blocked-file behavior.
- **Data/config:** [inferred] Per-user settings and optional local, dated reference-price data; file input is read-only and never uploaded. Market profile requires explicit field mapping and reports thresholds, sample size and coverage for constant spread, open equal to previous close, return-tail distribution and price plausibility. Without a licensed, dated instrument-specific baseline, show price comparison unavailable. Heuristics can misclassify data.
- **Packaging:** [inferred] Version-matched handler and worker, installer with opt-in per-user registration, warning before changing registration, and clean uninstaller restoring prior associations when safe. GitHub Releases with SHA256SUMS, then winget via manual Ava/Manager review; prepare optional Store MSIX and test handler registration, with a clear unsupported status if registration is unavailable. Apache-2.0 `LICENSE`, SPDX headers and third-party `THIRD_PARTY` inventory; audit provenance and dependencies. [measured] Ava's supplied, unverified policy says Store-certified MSIX is re-signed with a Microsoft certificate; EXE/MSI submissions require trusted pre-signing and self-signed certificates are rejected ([BRIEF-v5.md](/workspace/qol-portfolio/BRIEF-v5.md:18)). GitHub/winget remain unsigned with “not code-signed yet” and SmartScreen caveat unless SignPath Foundation approves. [measured] The supplied [SignPath terms](https://signpath.org/terms) are not re-verified: developer libraries/tools likely qualify for the reputation exemption; apps require verifiable reputation, 2FA, CI-built binaries, manual release approval, a Code signing policy naming authors/reviewers/approvers, privacy statement, uninstaller and warning before system-setting changes; publisher displays SignPath Foundation; applying is the Manager's call ([BRIEF-v5.md](/workspace/qol-portfolio/BRIEF-v5.md:18), [BRIEF.md](/workspace/qol-portfolio/BRIEF.md:87)).
- **Tests:** [inferred] Exercise multiple row groups, missing or misleading statistics, nested and nullable schemas, compression codecs, malformed footer, encrypted and oversized files, bounded decompression and incomplete scans. Test heuristic false positives and missing baselines. On Windows test stream-only access, cancellation, worker crash, handler conflicts, registration, upgrade and uninstall; compare alternatives on identical fixtures.
- **Size:** [guess] M–L, 12–20 developer days and 1,200–2,200 changed LOC, excluding baseline acquisition and external review.

## Licensing

[measured] The Manager requires Apache-2.0 for every project ([Amendment 10a](/workspace/qol-portfolio/BRIEF-v5.md:40)). Include the full Apache-2.0 text in `LICENSE`, use `SPDX-License-Identifier: Apache-2.0` on project source where practical, and open every README with “Made by Maria”. Each UI app includes an About screen with “Made by Maria”. Store copy, where applicable: “free and open source (Apache-2.0)”. The root `NOTICE` is exactly:

```text
Parquet viewer and preview
Copyright 2026 Maria (https://github.com/mariyaholic)

Made by Maria. This product includes software developed by Maria.
```

[inferred] MIT, BSD, ISC, Apache-2.0 and Zlib dependencies are compatible subject to their terms; retain required third-party notices in `THIRD_PARTY` so the required root template stays exact. Audit all code, transitive dependencies and assets; do not reuse GPL code such as mukunku/ParquetViewer. Rights and obligations still require review.

[measured] The [SignPath application draft](/workspace/qol-portfolio/SIGNPATH_APPLICATION_DRAFT.md:1) is draft only; a public repository and an existing public release are prerequisites and the Manager decides publication/application. Apache-2.0 preserves eligibility; no application or publishing is authorized by this architecture.

## Open questions

- [guess] Re-check the preview-handler landscape before release; the absence found in the comparison is only a search result.
- [guess] Which nested types, codecs, encryption and file sizes must the first release support?
- [guess] Does Store MSIX support the required preview registration on target Windows versions, and how should conflicts with existing handlers be resolved?
