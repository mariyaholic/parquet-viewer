#!/bin/sh
# SPDX-License-Identifier: Apache-2.0
set -eu
cd "$(dirname "$0")/.."
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
cp LICENSE NOTICE THIRD_PARTY.md "$DIST_DIR/"
(cd "$DIST_DIR" && sha256sum parquet_preview_windows.dll parquet-preview-worker.exe parquet-preview-cli.exe LICENSE NOTICE THIRD_PARTY.md > SHA256SUMS)
