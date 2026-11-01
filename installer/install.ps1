# SPDX-License-Identifier: Apache-2.0
# Conventional install entry point for the Parquet preview handler.
#
# Thin wrapper so an extracted release zip exposes the standard install.ps1 name
# next to the binaries. The implementation (conflict warning, backup and
# rollback) lives in Register-ParquetPreview.ps1 beside this file; any arguments
# (for example -DllPath, -Force or -Yes) pass straight through.
$ErrorActionPreference = 'Stop'
$LASTEXITCODE = 0
& (Join-Path $PSScriptRoot 'Register-ParquetPreview.ps1') @args
exit $LASTEXITCODE
