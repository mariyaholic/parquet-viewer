# SPDX-License-Identifier: Apache-2.0
# Conventional uninstall entry point for the Parquet preview handler.
#
# Equivalent to 'Register-ParquetPreview.ps1 -Unregister'; extra arguments (for
# example -DllPath or -Yes) pass straight through.
$ErrorActionPreference = 'Stop'
$LASTEXITCODE = 0
& (Join-Path $PSScriptRoot 'Register-ParquetPreview.ps1') -Unregister @args
exit $LASTEXITCODE
