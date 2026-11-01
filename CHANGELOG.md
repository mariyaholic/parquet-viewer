# Changelog

All notable changes are documented in this file, following Keep a Changelog.

## [Unreleased]

- Copy Explorer streams on their owning thread within a bounded memory budget.
- Preserve shared registry keys during uninstall, and include third-party notices in release output.
- Deliver releases as a single `parquet-preview-<version>-x64.zip` holding the binaries, licence documents and per-user `install.ps1`/`uninstall.ps1` registration scripts.
- Read the preview edit control's full text in the end-to-end harness, so the closing credit line is asserted instead of truncated.

## [0.1.0]

- Add a local, read-only Parquet preview handler, worker and diagnostics CLI.
- Register the preview handler for the current user and preserve existing associations.
