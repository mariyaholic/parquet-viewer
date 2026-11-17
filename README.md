Made by Maria

# Parquet viewer and preview

Hello! This is my viewer for local Parquet files, right inside Windows Explorer.
It shows the schema, the row-group information and what was scanned. I like
that it always tells me exactly what was scanned and what was not, and I hope
it helps you peek at your files as much as it helps me.

This product is free and open source (Apache-2.0). See [LICENSE](LICENSE) and [NOTICE](NOTICE).

Status: v1 implementation. Releases are not code-signed yet.

## Get it

Releases are published as `parquet-preview-<version>-x64.zip`. Extract the zip
and run the per-user install script from the extracted folder:

```text
powershell -ExecutionPolicy Bypass -File install.ps1
```

Registration is per-user, so it asks for no administrator rights. Restart
Explorer afterwards and your `.parquet` files should get a preview pane. To
remove it again, run `uninstall.ps1` the same way.

## What it does for you

- A footer with the schema, the row groups and a row count. The counts stay
  labelled until a full scan verifies them.
- Date and timestamp bounds that say `complete`, `partial` or `unavailable`,
  with a reason for each column.
- A bounded sample that renders nested structs, lists, maps, decimals, dates,
  timestamps and binary values safely.
- An optional full scan with row, size and time budgets. A cancelled or
  skipped scan never claims exactness.
- An About dialog (F1 in the preview pane) with "Made by Maria" and the
  "free and open source (Apache-2.0)" notice.

I want the boundaries of this one to be plain, so nothing surprises you. There
is no SQL or full-file editing, and there is no data-network access, no
uploads and no telemetry. MT5/MetaTrader support, if configured later, is
strictly read-only: this project places no orders and makes no trading calls.
It is not affiliated with MetaQuotes; MetaTrader 5 and MT5 are trademarks of
MetaQuotes Ltd.

## Documentation

- [Start here: desktop user guide](docs/USER-GUIDE.md) — opening, browsing,
  searching, copying and appearance.
- In the app, choose **Help** for topics or the bundled offline guide; **F1**
  opens Quick help.

## Standalone desktop preview

I also built a native Windows desktop companion, dressed in **Paper, Ink,
PC-98, Doloris, Sumimi and Marigold** themes. Extract its portable developer
ZIP and run `parquet-desktop.exe`; no COM registration is needed. It is a
bounded, read-only paged browser with cross-page literal search, not an
editable spreadsheet.
[Large-file browsing and search limits](docs/LARGE-FILE-BROWSING.md).

## Privacy and offline behavior

Files and optional settings stay local. The application does not access the
network or send telemetry. I would not have it any other way.

## Licence

Apache-2.0; see [LICENSE](LICENSE), [NOTICE](NOTICE) and [THIRD_PARTY.md](THIRD_PARTY.md).

## Everything else

The fine detail lives in [Parquet Viewer's guide](https://mariyaholic.github.io/docs/parquet-viewer/)
on my site:

- [What the viewer shows](https://mariyaholic.github.io/docs/parquet-viewer/#scope) —
  coverage labels, date bounds, the optional scan and the About dialog
- [The workspace crates](https://mariyaholic.github.io/docs/parquet-viewer/#workspace)
  and the architecture documents
- [Build and test from source](https://mariyaholic.github.io/docs/parquet-viewer/#build),
  including the Windows release scripts
- [The headless command-line tool](https://mariyaholic.github.io/docs/parquet-viewer/#headless)
- [Market diagnostics](https://mariyaholic.github.io/docs/parquet-viewer/#market)
  and the settings file
- [Registering the Explorer handler](https://mariyaholic.github.io/docs/parquet-viewer/#register) —
  scripts, backups and version matching
- [Known limitations](https://mariyaholic.github.io/docs/parquet-viewer/#limits)

Made by Maria. Free and open source (Apache-2.0).

Thank you for reading. ♡
