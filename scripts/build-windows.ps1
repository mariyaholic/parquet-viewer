# SPDX-License-Identifier: Apache-2.0
#
# Build the Windows release payload into $DIST_DIR, then bundle it into the
# single release archive parquet-preview-<Version>-x64.zip. -Version defaults to
# a dev placeholder; the release workflow passes the git tag.
param([string]$Version = '0.0.0-dev')
$ErrorActionPreference = 'Stop'
Set-Location (Join-Path $PSScriptRoot '..')
if (-not $env:CARGO_TARGET_DIR) { $env:CARGO_TARGET_DIR = Join-Path (Get-Location) 'target' }
if (-not $env:DIST_DIR) { $env:DIST_DIR = Join-Path (Get-Location) 'dist' }
$env:SOURCE_DATE_EPOCH = git log -1 --format=%ct
$env:RUSTFLAGS = "$env:RUSTFLAGS --remap-path-prefix=$(Get-Location)=."
cargo build --offline --locked --release -p parquet-preview-windows -p parquet-preview-worker -p parquet-preview-cli
if ($LASTEXITCODE -ne 0) { throw 'Cargo build failed' }
New-Item -ItemType Directory -Force $env:DIST_DIR | Out-Null
$binaries = @('parquet_preview_windows.dll', 'parquet-preview-worker.exe', 'parquet-preview-cli.exe')
foreach ($name in $binaries) { Copy-Item (Join-Path $env:CARGO_TARGET_DIR "release/$name") $env:DIST_DIR -Force }
$documents = @('LICENSE', 'NOTICE', 'THIRD_PARTY.md', 'THIRD_PARTY_LICENSES.txt')
foreach ($name in $documents) { Copy-Item $name $env:DIST_DIR -Force }
# The per-user registration entry points and their guide ship next to the
# binaries so the extracted zip registers in place (installer/README.md).
$scripts = @('Register-ParquetPreview.ps1', 'install.ps1', 'uninstall.ps1')
foreach ($name in $scripts) { Copy-Item (Join-Path 'installer' $name) $env:DIST_DIR -Force }
Copy-Item (Join-Path 'installer' 'README.md') (Join-Path $env:DIST_DIR 'INSTALL.md') -Force
$payload = $binaries + $documents + $scripts + @('INSTALL.md')
$lines = foreach ($name in $payload) { "$( (Get-FileHash (Join-Path $env:DIST_DIR $name) -Algorithm SHA256).Hash.ToLowerInvariant() )  $name" }
[System.IO.File]::WriteAllLines((Join-Path $env:DIST_DIR 'SHA256SUMS'), $lines, [System.Text.UTF8Encoding]::new($false))
# One versioned archive per release; it carries its own checksums and avoids the
# zip including itself.
$archive = Join-Path $env:DIST_DIR "parquet-preview-$Version-x64.zip"
if (Test-Path -LiteralPath $archive) { Remove-Item -LiteralPath $archive -Force }
Compress-Archive -Path (($payload + 'SHA256SUMS') | ForEach-Object { Join-Path $env:DIST_DIR $_ }) -DestinationPath $archive -Force
Write-Host "Wrote $archive"
