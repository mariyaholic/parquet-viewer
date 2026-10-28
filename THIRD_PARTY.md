# Third-party software

The project is Apache-2.0. This file records the third-party packages used by
the shipped targets and by the build. It is generated from `Cargo.lock` and
`cargo metadata`; the exact versions are pinned in `Cargo.lock`.

Audit date: 2026-10-28. Commands used:

```text
cargo metadata --format-version 1
cargo tree --target x86_64-pc-windows-msvc --workspace --edges normal,build
cargo tree --target x86_64-unknown-linux-gnu --workspace --edges normal,build
```

No GPL/AGPL code is linked into the shipped Windows binaries. The reader is the
Apache-2.0 `parquet` crate from Apache Arrow; no code was taken from
mukunku/ParquetViewer or other GPL viewers.

Licence expressions below are the upstream `license` fields. Where a package
offers a choice, this project relies on the MIT, Apache-2.0, BSD or Zlib option.
`Unlicense OR MIT` packages are used under MIT. `BSD-2-Clause` is recorded
separately and is permissive.

## Direct runtime dependencies

| Package | Licence | Use |
| --- | --- | --- |
| parquet 58.4.1 | Apache-2.0 | Read-only Parquet decoding |
| arrow-array 58.4.1 | Apache-2.0 AND MIT | Arrow sample/scan arrays |
| arrow-buffer 58.4.1 | Apache-2.0 | Fixture generation buffers |
| arrow-schema 58.4.1 | Apache-2.0 | Arrow schema/data types |
| serde 1.0.229 | MIT OR Apache-2.0 | Report/settings serialization |
| serde_json 1.0.154 | MIT OR Apache-2.0 | JSON protocol and settings |
| thiserror 2.0.21 | MIT OR Apache-2.0 | Error types |
| bytes 1.12.1 | MIT | Bounded `ChunkReader` buffers |

Compression backends enabled by the `parquet` features: `snap`
(BSD-3-Clause), `brotli`/`brotli-decompressor` (BSD-3-Clause/MIT),
`flate2` (MIT/Apache-2.0), `lz4_flex` (MIT), `zstd`/`zstd-safe`/`zstd-sys`
(MIT/BSD-3-Clause), `zlib-rs` (Zlib).

## Transitive packages in the Windows/Linux build graph

| Package | Version | Licence |
| --- | --- | --- |
| ahash | 0.8.12 | MIT OR Apache-2.0 |
| alloc-no-stdlib | 2.0.4 | BSD-3-Clause |
| alloc-stdlib | 0.2.4 | BSD-3-Clause |
| arrow-data | 58.4.1 | Apache-2.0 |
| arrow-ipc | 58.4.1 | Apache-2.0 |
| arrow-select | 58.4.1 | Apache-2.0 |
| autocfg | 1.5.1 | Apache-2.0 OR MIT |
| base64 | 0.22.1 | MIT OR Apache-2.0 |
| bitflags | 2.13.2 | MIT OR Apache-2.0 |
| brotli | 8.0.4 | BSD-3-Clause AND MIT |
| brotli-decompressor | 5.0.3 | BSD-3-Clause/MIT |
| byteorder | 1.5.0 | Unlicense OR MIT (used as MIT) |
| cc | 1.10.0 | MIT OR Apache-2.0 |
| cfg-if | 1.0.5 | MIT OR Apache-2.0 |
| chrono | 0.4.45 | MIT OR Apache-2.0 |
| find-msvc-tools | 0.1.15 | MIT OR Apache-2.0 |
| flatbuffers | 25.12.19 | Apache-2.0 |
| flate2 | 1.1.10 | MIT OR Apache-2.0 |
| getrandom | 0.3.4 | MIT OR Apache-2.0 |
| half | 2.7.1 | MIT OR Apache-2.0 |
| hashbrown | 0.17.1 | MIT OR Apache-2.0 |
| iana-time-zone | 0.1.65 | MIT OR Apache-2.0 |
| integer-encoding | 3.0.4 | MIT |
| itoa | 1.0.18 | MIT OR Apache-2.0 |
| jobserver | 0.1.35 | MIT OR Apache-2.0 |
| libc | 0.2.191 | MIT OR Apache-2.0 |
| libm | 0.2.16 | MIT |
| lz4_flex | 0.13.1 | MIT |
| memchr | 2.8.3 | Unlicense OR MIT (used as MIT) |
| num-bigint | 0.4.8 | MIT OR Apache-2.0 |
| num-complex | 0.4.6 | MIT OR Apache-2.0 |
| num-integer | 0.1.47 | MIT OR Apache-2.0 |
| num-traits | 0.2.19 | MIT OR Apache-2.0 |
| once_cell | 1.21.4 | MIT OR Apache-2.0 |
| ordered-float | 3.9.2 | MIT |
| paste | 1.0.15 | MIT OR Apache-2.0 |
| pkg-config | 0.3.34 | MIT OR Apache-2.0 |
| proc-macro2 | 1.0.107 | MIT OR Apache-2.0 |
| quote | 1.0.47 | MIT OR Apache-2.0 |
| rustc_version | 0.4.1 | MIT OR Apache-2.0 |
| semver | 1.0.28 | MIT OR Apache-2.0 |
| seq-macro | 0.3.6 | MIT OR Apache-2.0 |
| serde_core | 1.0.229 | MIT OR Apache-2.0 |
| serde_derive | 1.0.229 | MIT OR Apache-2.0 |
| shlex | 2.0.1 | MIT OR Apache-2.0 |
| snap | 1.1.2 | BSD-3-Clause |
| syn | 2.0.119 / 3.0.9 | MIT OR Apache-2.0 |
| thiserror-impl | 2.0.21 | MIT OR Apache-2.0 |
| thrift | 0.23.0 | Apache-2.0 |
| twox-hash | 2.1.5 | MIT |
| unicode-ident | 1.0.27 | (MIT OR Apache-2.0) AND Unicode-3.0 (build-time only) |
| uuid | 1.18.1 | Apache-2.0 OR MIT |
| version_check | 0.9.5 | MIT/Apache-2.0 |
| windows-link | 0.2.1 | MIT OR Apache-2.0 |
| zerocopy | 0.8.63 | BSD-2-Clause OR Apache-2.0 OR MIT |
| zerocopy-derive | 0.8.63 | BSD-2-Clause OR Apache-2.0 OR MIT |
| zlib-rs | 0.6.8 | Zlib |
| zmij | 1.0.23 | MIT |
| zstd | 0.13.3 | MIT |
| zstd-safe | 7.3.0 | BSD-3-Clause |
| zstd-sys | 2.1.2+zstd.1.5.7 | BSD-3-Clause |

## Build-only and non-shipping packages

`unicode-ident` is pulled in by the `proc-macro2`/`syn` build chain; it is
compiled into build-time proc-macro libraries and is not distributed in the
handler or worker binaries. It offers MIT/Apache-2.0 plus the permissive
Unicode-3.0 data licence; this combination is recorded for review because the
project brief names only MIT/BSD/ISC/Apache-2.0/Zlib.

Cargo metadata for all targets also names wasm/wasi packages (`js-sys`,
`wasm-bindgen*`, `wasi`, `wasip2`, `wit-bindgen`, `r-efi`) and `windows-*`
helper crates. They are target-specific dependencies of upstream crates and are
not compiled into the x86_64 Windows or Linux builds. `tiny-keccak` (CC0-1.0)
appears only in an all-target metadata view; it is not in the Windows build tree.

## Verifying

```text
cargo tree --target x86_64-pc-windows-msvc --workspace --edges normal,build
cargo test --workspace
```

If a dependency changes licence or a new dependency is added, update this file
and re-run the review before release.
