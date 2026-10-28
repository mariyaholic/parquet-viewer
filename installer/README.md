# Installer (v1)

This is an opt-in, per-user registration script. It is not a packaged MSI or
MSIX; see [ROADMAP.md](../ROADMAP.md). Run it from your own account, not an
elevated administrator shell, because registration writes
`HKCU\Software\Classes`.

## Bundle layout

Build on 64-bit Windows with the MSVC toolchain, then place these files together
in one directory:

```text
parquet_preview_windows.dll
parquet-preview-worker.exe
```

The handler resolves the worker next to the DLL. The worker and handler must be
from the same build. `PARQUET_PREVIEW_WORKER` may point at a worker for
development, but packaged builds should use the sibling file.

## Register

```text
powershell -ExecutionPolicy Bypass -File installer\Register-ParquetPreview.ps1 -DllPath <path>\parquet_preview_windows.dll
```

The script warns before changing `.parquet` associations. If another handler is
present it asks before replacing it, and the DLL backs up the previous value
under `HKCU\Software\ParquetPreview\Backup`. Non-interactive automation can use
`-Yes`; use `-Force` only after reviewing the conflict warning.

Registration writes:

```text
HKCU\Software\Classes\.parquet\ShellEx\{8895B1C6-B41F-4C1C-A562-0D564250836F}
HKCU\Software\Classes\CLSID\{7F2C9E4A-2B6D-4B7E-9C3A-5D1E8A0F6B21}\InprocServer32
```

Restart Explorer after registration.

## Unregister

```text
powershell -ExecutionPolicy Bypass -File installer\Register-ParquetPreview.ps1 -Unregister -DllPath <path>\parquet_preview_windows.dll
```

The previous association is restored only when the current association is still
this handler. If another product took over `.parquet` after installation, the
uninstaller leaves it alone and removes only this handler's CLSID.

## Caveats

- Not code-signed yet; Windows SmartScreen may warn.
- Per-user registration may be blocked by policy, or a preview-handler conflict
  may prevent Explorer from using this handler. The script reports `regsvr32`
  failures; there is not yet a Store/MSIX registration path.
- The worker is assigned to a job object with one-process, 512 MiB and
  kill-on-close limits. That is resource control, not a security sandbox.
- No network access, telemetry or uploads are performed by this project.
