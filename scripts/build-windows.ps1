# SPDX-License-Identifier: Apache-2.0
$ErrorActionPreference = 'Stop'
Set-Location (Join-Path $PSScriptRoot '..')
if (-not $env:CARGO_TARGET_DIR) { $env:CARGO_TARGET_DIR = Join-Path (Get-Location) 'target' }
if (-not $env:DIST_DIR) { $env:DIST_DIR = Join-Path (Get-Location) 'dist' }
$env:SOURCE_DATE_EPOCH = git log -1 --format=%ct
$env:RUSTFLAGS = "$env:RUSTFLAGS --remap-path-prefix=$(Get-Location)=."
cargo build --offline --locked --release -p parquet-preview-windows -p parquet-preview-worker -p parquet-preview-cli
if ($LASTEXITCODE -ne 0) { throw 'Cargo build failed' }
New-Item -ItemType Directory -Force $env:DIST_DIR | Out-Null
$names = @('parquet_preview_windows.dll', 'parquet-preview-worker.exe', 'parquet-preview-cli.exe')
foreach ($name in $names) { Copy-Item (Join-Path $env:CARGO_TARGET_DIR "release/$name") $env:DIST_DIR -Force }
$documents = @('LICENSE', 'NOTICE', 'THIRD_PARTY.md')
foreach ($name in $documents) { Copy-Item $name $env:DIST_DIR -Force }
$names += $documents
$lines = foreach ($name in $names) { "$( (Get-FileHash (Join-Path $env:DIST_DIR $name) -Algorithm SHA256).Hash.ToLowerInvariant() )  $name" }
[System.IO.File]::WriteAllLines((Join-Path $env:DIST_DIR 'SHA256SUMS'), $lines, [System.Text.UTF8Encoding]::new($false))
