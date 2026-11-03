# SPDX-License-Identifier: Apache-2.0
#
# Offline generator for THIRD_PARTY_LICENSES.txt.
#
# This is the dependency-free counterpart to `cargo about generate about.hbs`.
# It walks the shipped (x86_64-pc-windows-gnu) dependency graph via
# `cargo metadata` and concatenates the licence text of every third-party
# crate, so the MIT/BSD notice obligations are met in the binary distribution.
#
# It deliberately needs no network and no `cargo install`: this repository's
# opencode.json denies both, and AGENTS.md forbids installing toolchains.
#
# Usage:
#   pwsh scripts/generate-third-party-licenses.ps1          # rewrite the file
#   pwsh scripts/generate-third-party-licenses.ps1 -Check   # fail on drift
param(
    [switch]$Check
)

$ErrorActionPreference = 'Stop'
Set-Location (Join-Path $PSScriptRoot '..')
$outputPath = Join-Path (Get-Location) 'THIRD_PARTY_LICENSES.txt'
$target = 'x86_64-pc-windows-gnu'

$raw = & cargo metadata --offline --locked --format-version 1 --filter-platform $target
if ($LASTEXITCODE -ne 0) { throw "cargo metadata failed (exit $LASTEXITCODE)" }
$metadata = ($raw -join "`n") | ConvertFrom-Json

$licenseFilePattern = '^(LICENSE|LICENCE|COPYING|COPYRIGHT|NOTICE|UNLICENSE)'
$licenseFileOptions = [System.Text.RegularExpressions.RegexOptions]::IgnoreCase -bor
    [System.Text.RegularExpressions.RegexOptions]::CultureInvariant
$licenseFileRegex = [regex]::new($licenseFilePattern, $licenseFileOptions)
$sections = New-Object System.Collections.Generic.List[object]

foreach ($pkg in $metadata.packages) {
    if ($pkg.name -like 'parquet-preview-*') { continue }
    if (-not $pkg.source -or $pkg.source -notlike 'registry+*') { continue }

    $dir = Split-Path -Parent $pkg.manifest_path
    $files = New-Object System.Collections.Generic.List[System.IO.FileInfo]

    if ($pkg.license_file) {
        $explicit = Join-Path $dir ($pkg.license_file -replace '/', [IO.Path]::DirectorySeparatorChar)
        if (Test-Path -LiteralPath $explicit -PathType Leaf) {
            $files.Add((Get-Item -LiteralPath $explicit))
        }
    }
    if ($files.Count -eq 0) {
        # Sort ordinally. Culture-aware Sort-Object orders LICENSE.txt before
        # LICENSE-MIT on Windows PowerShell 5.1 but after it on PowerShell 7, so
        # -Check could never agree across the engines that generate and verify
        # the bundle. Ordinal order is identical everywhere.
        $matched = @(
            Get-ChildItem -LiteralPath $dir -File -ErrorAction SilentlyContinue |
                Where-Object { $licenseFileRegex.IsMatch($_.Name) }
        )
        $matchedNames = [string[]]@($matched | ForEach-Object { $_.Name })
        if ($matchedNames.Length -gt 0) {
            [Array]::Sort($matchedNames, [System.StringComparer]::Ordinal)
            foreach ($name in $matchedNames) {
                $files.Add(($matched | Where-Object { [StringComparer]::Ordinal.Equals($_.Name, $name) })[0])
            }
        }
    }

    $chunks = New-Object System.Collections.Generic.List[string]
    foreach ($file in $files) {
        $text = Get-Content -LiteralPath $file.FullName -Raw -ErrorAction SilentlyContinue
        if ($null -eq $text) { continue }
        $text = ($text -replace "`r`n", "`n") -replace "`r", "`n"
        $chunks.Add($text.TrimEnd() + "`n")
    }

    $sections.Add([PSCustomObject]@{
            Name    = $pkg.name
            Version = $pkg.version
            License = $pkg.license
            Text    = ($chunks -join "`n")
        })
}

# Package order must be ordinal too: culture-aware sorting moves
# windows_x86_64_gnu before windows-link, unlike the checked-in inventory.
# Keep both package and licence-file ordering independent of host culture.
$ordered = [object[]]$sections.ToArray()
[Array]::Sort($ordered, [System.Collections.Generic.Comparer[object]]::Create({
    param($left, $right)
    $nameOrder = [StringComparer]::Ordinal.Compare($left.Name, $right.Name)
    if ($nameOrder -ne 0) { return $nameOrder }
    return [StringComparer]::Ordinal.Compare($left.Version, $right.Version)
}))
$rule = '=' * 78
$sb = New-Object System.Text.StringBuilder
foreach ($line in @(
        '# Third-party licences',
        '#',
        '# Generated from the x86_64-pc-windows-gnu dependency graph. Do not edit',
        '# by hand: run scripts/generate-third-party-licenses.ps1 when Cargo.lock',
        "# changes (or 'cargo about generate about.hbs' with cargo-about).",
        '#',
        '# Shipped next to LICENSE and NOTICE in every distribution; the package',
        '# inventory is in THIRD_PARTY.md.'
    )) { [void]$sb.Append($line).Append("`n") }
[void]$sb.Append("`n")

foreach ($s in $ordered) {
    [void]$sb.Append($rule).Append("`n")
    [void]$sb.Append("$($s.Name) $($s.Version)").Append("`n")
    [void]$sb.Append("License: $($s.License)").Append("`n")
    [void]$sb.Append($rule).Append("`n")
    if ([string]::IsNullOrWhiteSpace($s.Text)) {
        [void]$sb.Append("(no licence file in the crate; SPDX: $($s.License))").Append("`n")
    }
    else {
        [void]$sb.Append($s.Text)
        if (-not $s.Text.EndsWith("`n")) { [void]$sb.Append("`n") }
    }
    [void]$sb.Append("`n")
}

$content = $sb.ToString()
$utf8 = New-Object System.Text.UTF8Encoding($false)

if ($Check) {
    $existing = ''
    if (Test-Path -LiteralPath $outputPath) {
        $existing = (Get-Content -LiteralPath $outputPath -Raw) -replace "`r`n", "`n"
    }
    if ($existing -ne $content) {
        Write-Error 'THIRD_PARTY_LICENSES.txt is out of date; run scripts/generate-third-party-licenses.ps1'
        exit 1
    }
    Write-Output 'THIRD_PARTY_LICENSES.txt is up to date'
    exit 0
}

[IO.File]::WriteAllText($outputPath, $content, $utf8)
Write-Output "wrote THIRD_PARTY_LICENSES.txt ($($ordered.Count) packages)"
