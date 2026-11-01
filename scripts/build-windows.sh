#!/bin/sh
# SPDX-License-Identifier: Apache-2.0
#
# Build the Windows release payload into $DIST_DIR, then bundle it into the
# single release archive parquet-preview-$VERSION-x64.zip. VERSION defaults to a
# dev placeholder; the release workflow passes the git tag.
set -eu
cd "$(dirname "$0")/.."
: "${VERSION:=0.0.0-dev}"
: "${CARGO_TARGET_DIR:=$PWD/target}"
: "${DIST_DIR:=$PWD/dist}"
export CARGO_TARGET_DIR CARGO_NET_OFFLINE=true
SOURCE_DATE_EPOCH=$(git log -1 --format=%ct)
export SOURCE_DATE_EPOCH
RUSTFLAGS="${RUSTFLAGS:-} --remap-path-prefix=$PWD=."
export RUSTFLAGS
cargo build --offline --locked --release --target x86_64-pc-windows-gnu -p parquet-preview-windows -p parquet-preview-worker -p parquet-preview-cli
mkdir -p "$DIST_DIR"
cp "$CARGO_TARGET_DIR/x86_64-pc-windows-gnu/release/parquet_preview_windows.dll" "$CARGO_TARGET_DIR/x86_64-pc-windows-gnu/release/parquet-preview-worker.exe" "$CARGO_TARGET_DIR/x86_64-pc-windows-gnu/release/parquet-preview-cli.exe" "$DIST_DIR/"
cp LICENSE NOTICE THIRD_PARTY.md THIRD_PARTY_LICENSES.txt "$DIST_DIR/"
cp installer/Register-ParquetPreview.ps1 installer/install.ps1 installer/uninstall.ps1 "$DIST_DIR/"
cp installer/README.md "$DIST_DIR/INSTALL.md"
(cd "$DIST_DIR" && sha256sum parquet_preview_windows.dll parquet-preview-worker.exe parquet-preview-cli.exe LICENSE NOTICE THIRD_PARTY.md THIRD_PARTY_LICENSES.txt Register-ParquetPreview.ps1 install.ps1 uninstall.ps1 INSTALL.md > SHA256SUMS)
rm -f "$DIST_DIR/parquet-preview-$VERSION-x64.zip"
(cd "$DIST_DIR" && zip -r "parquet-preview-$VERSION-x64.zip" parquet_preview_windows.dll parquet-preview-worker.exe parquet-preview-cli.exe LICENSE NOTICE THIRD_PARTY.md THIRD_PARTY_LICENSES.txt Register-ParquetPreview.ps1 install.ps1 uninstall.ps1 INSTALL.md SHA256SUMS)
echo "Wrote $DIST_DIR/parquet-preview-$VERSION-x64.zip"
