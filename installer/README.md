# Installer (v1)

This is an opt-in, per-user registration script. It is not a packaged MSI or
MSIX; see [ROADMAP.md](../ROADMAP.md). Run it from your own account, not an
elevated administrator shell, because registration writes
`HKCU\Software\Classes`.

## Release layout (zip)

Releases are delivered as a single zip, `parquet-preview-<version>-x64.zip`.
Extract it anywhere and register in place:

```text
parquet_preview_windows.dll
parquet-preview-worker.exe
parquet-preview-cli.exe
LICENSE
NOTICE
THIRD_PARTY.md
THIRD_PARTY_LICENSES.txt
SHA256SUMS                      # checksums of every file above and below
INSTALL.md                      # this guide (copied from installer/README.md)
install.ps1                     # register entry point
uninstall.ps1                   # unregister entry point
Register-ParquetPreview.ps1     # the implementation both scripts call
```

The handler resolves the worker next to the DLL, so the two must be from the
same build. `PARQUET_PREVIEW_WORKER` may point at a worker for development, but
release builds use the sibling file (the override is compiled out of release
binaries).

## Register

From the extracted folder:

```text
powershell -ExecutionPolicy Bypass -File install.ps1
```

`install.ps1` is a thin wrapper over `Register-ParquetPreview.ps1`; either name
works and passes the same arguments. The script warns before changing `.parquet`
associations. If another handler is present it asks before replacing it, and the
DLL backs up the previous value under `HKCU\Software\ParquetPreview\Backup`.
Non-interactive automation can use `-Yes`; use `-Force` only after reviewing the
conflict warning.

Registration writes:

```text
HKCU\Software\Classes\.parquet\ShellEx\{8895B1C6-B41F-4C1C-A562-0D564250836F}
HKCU\Software\Classes\CLSID\{7F2C9E4A-2B6D-4B7E-9C3A-5D1E8A0F6B21}\InprocServer32
```

Restart Explorer after registration.

## Unregister

```text
powershell -ExecutionPolicy Bypass -File uninstall.ps1
```

Equivalent to `Register-ParquetPreview.ps1 -Unregister`. The previous
association is restored only when the current association is still this handler.
If another product took over `.parquet` after installation, the uninstaller
leaves it alone and removes only this handler's CLSID.

## Why a zip and a script, not an installer

A per-user, no-elevation shell extension is conventionally delivered as a zip you
extract and register with a script; nothing is written outside the current
user's registry. A packaged installer (MSI or MSIX) with its own uninstall entry,
signing and version-matched upgrade path is deliberately out of v1 scope and is
tracked in [ROADMAP.md](../ROADMAP.md).

## Caveats

- Not code-signed yet; Windows SmartScreen may warn.
- Per-user registration may be blocked by policy, or a preview-handler conflict
  may prevent Explorer from using this handler. The script reports `regsvr32`
  failures; there is not yet a Store/MSIX registration path.
- The worker is assigned to a job object with one-process, 512 MiB and
  kill-on-close limits. That is resource control, not a security sandbox.
- No network access, telemetry or uploads are performed by this project.
