# SPDX-License-Identifier: Apache-2.0
# Opt-in per-user registration for the Parquet preview handler.
# Written by Maria.

param(
    [string]$DllPath,
    [switch]$Unregister,
    [switch]$Force,
    [switch]$Yes
)

$ErrorActionPreference = "Stop"
$Clsid = "{7F2C9E4A-2B6D-4B7E-9C3A-5D1E8A0F6B21}"
$PreviewGuid = "{8895B1C6-B41F-4C1C-A562-0D564250836F}"
$AssociationKey = "HKCU:\Software\Classes\.parquet\ShellEx\$PreviewGuid"

function Resolve-DllPath {
    param([string]$Requested)
    if ($Requested) {
        if (-not (Test-Path -LiteralPath $Requested -PathType Leaf)) {
            throw "DLL not found: $Requested"
        }
        return (Resolve-Path -LiteralPath $Requested).Path
    }
    $candidates = @(
        (Join-Path $PSScriptRoot "..\target\release\parquet_preview_windows.dll"),
        (Join-Path $PSScriptRoot "..\target\debug\parquet_preview_windows.dll"),
        (Join-Path $PSScriptRoot "parquet_preview_windows.dll")
    )
    foreach ($candidate in $candidates) {
        if (Test-Path -LiteralPath $candidate -PathType Leaf) {
            return (Resolve-Path -LiteralPath $candidate).Path
        }
    }
    throw "Pass -DllPath with the path to parquet_preview_windows.dll."
}

function Get-CurrentHandler {
    try {
        $item = Get-Item -LiteralPath $AssociationKey -ErrorAction Stop
        return [string]$item.GetValue("")
    } catch {
        return $null
    }
}

function Invoke-RegSvr32 {
    param([string]$Path, [switch]$Unregister)
    $arguments = @("/s")
    if ($Unregister) { $arguments += "/u" }
    $arguments += $Path
    $process = Start-Process -FilePath "regsvr32.exe" -ArgumentList $arguments -Wait -PassThru -NoNewWindow
    if ($process.ExitCode -ne 0) {
        throw "regsvr32 failed with exit code $($process.ExitCode). Registration may be unavailable or blocked."
    }
}

if ($Unregister) {
    Write-Warning "This removes the per-user .parquet preview handler and restores the previous association only if it has not been changed since installation."
    if (-not $Yes) {
        $answer = Read-Host "Unregister the Parquet preview handler? [y/N]"
        if ($answer -notmatch "^(y|yes)$") { Write-Host "Cancelled."; exit 1 }
    }
    $path = Resolve-DllPath -Requested $DllPath
    Invoke-RegSvr32 -Path $path -Unregister
    Write-Host "Unregistered. Restart Explorer to see the change."
    exit 0
}

$dll = Resolve-DllPath -Requested $DllPath
$worker = Join-Path (Split-Path -Parent $dll) "parquet-preview-worker.exe"
if (-not (Test-Path -LiteralPath $worker -PathType Leaf)) {
    Write-Warning "Worker not found next to the DLL: $worker. The handler will fail until parquet-preview-worker.exe is installed beside it."
}

$existing = Get-CurrentHandler
Write-Host "This changes your per-user .parquet preview handler."
Write-Host "  DLL: $dll"
if ($existing -and $existing -ne $Clsid) {
    Write-Warning "An existing preview handler is registered: $existing"
    Write-Warning "It will be backed up and restored on uninstall only if the association is not changed afterwards."
    if (-not $Force -and -not $Yes) {
        $answer = Read-Host "Replace it? [y/N]"
        if ($answer -notmatch "^(y|yes)$") { Write-Host "Cancelled."; exit 1 }
    }
} elseif (-not $Yes) {
    $answer = Read-Host "Register the preview handler for .parquet? [y/N]"
    if ($answer -notmatch "^(y|yes)$") { Write-Host "Cancelled."; exit 1 }
}

# The DLL refuses to overwrite a foreign handler unless this explicit consent is
# present. The warning and prompt above are the consent step.
if ($existing -and $existing -ne $Clsid) {
    $env:PARQUET_PREVIEW_ALLOW_OVERWRITE = "1"
}

try {
    Invoke-RegSvr32 -Path $dll
} finally {
    Remove-Item Env:\PARQUET_PREVIEW_ALLOW_OVERWRITE -ErrorAction SilentlyContinue
}

Write-Host "Registered. Restart Explorer (or sign out and back in) to load the preview handler."
Write-Host "The handler is read-only and performs no network access. Not code-signed yet; SmartScreen may warn."
