# Roadmap (not v1)

These items are deliberately out of the v1 scope described in
`docs/ARCHITECTURE.md`. None of them is implemented yet.

- Real Windows verification of COM activation, Explorer stream behaviour,
  registration conflicts, job assignment and cancellation; see docs/TESTING.md.
- Code signing (SignPath Foundation or another approved route) and release
  SHA256SUMS. Releases are "not code-signed yet".
- MSIX/Store packaging and its preview-handler registration support, including a
  clear unsupported path if Store MSIX cannot register the handler.
- Installer/updater with version-matched handler and worker, clean uninstall and
  association conflict handling beyond the current per-user script.
- ARM64 Windows build and host-bitness checks beyond the current 64-bit-only
  compile gate.
- Page-index-based virtual scrolling and a file-name aware initialization path
  for large files.
- Exact bounds for nested date/timestamp leaves; v1 covers top-level columns
  only and states the limitation.
- Licensed, dated instrument-specific price databases and configurable
  thresholds with per-field provenance. v1 only reads a local CSV baseline.
- Localization, accessibility polish, theme-aware colors and a richer About
  screen. The current About dialog is F1 in the preview pane.
- Fuzzing of the footer/statistics parsers and long-duration malformed-file
  testing.

Out of scope by design: SQL/querying, a standalone GUI viewer, writing or
modifying Parquet files, uploading data, telemetry, and any MT5 trading call.
