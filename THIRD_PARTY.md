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

The full licence texts of every package below are distributed in
[`THIRD_PARTY_LICENSES.txt`](THIRD_PARTY_LICENSES.txt), generated from the
`x86_64-pc-windows-gnu` dependency graph by
`scripts/generate-third-party-licenses.ps1` (offline, no tool install). It ships
next to `LICENSE`/`NOTICE` in every `dist/` bundle.

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
Unicode-3.0 data licence. The cached crate README identifies Unicode Character Database-derived tables and requires both licences; its complete Unicode notice is reproduced below.

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

## Offline audit and bundled notices

The following complete all-target inventory is generated from offline Cargo metadata; versions and licence expressions are upstream manifest declarations. `cargo deny --offline check licenses` checked these declarations and licence files with `deny.toml`, including separate Windows GNU and Linux target runs. Target-specific CC0-1.0 `tiny-keccak` 2.0.2 declares CC0 and includes its dedication; it is absent from the normal Windows/Linux trees. Unicode-3.0 and CC0 are permitted for this audit. The inventory includes alternatives outside the requested set: byteorder 1.5.0 and memchr 2.8.3 offer Unlicense; r-efi 5.3.0 and 6.0.0 offer LGPL-2.1-or-later; wasi 0.11.1, wasip2 1.0.4 and wit-bindgen 0.57.1 offer Apache-2.0 WITH LLVM-exception. These alternatives are not selected: use the MIT or plain Apache-2.0 option. Cargo-deny accepted the licence expressions on that basis; the r-efi/WASI packages are not in the normal Windows/Linux target graphs.

| Package | Version | Declared licence |
| --- | --- | --- |
| ahash | 0.8.12 | MIT OR Apache-2.0 |
| alloc-no-stdlib | 2.0.4 | BSD-3-Clause |
| alloc-stdlib | 0.2.4 | BSD-3-Clause |
| android_system_properties | 0.1.6 | MIT OR Apache-2.0 |
| arrow-array | 58.4.1 | Apache-2.0 AND MIT |
| arrow-buffer | 58.4.1 | Apache-2.0 |
| arrow-data | 58.4.1 | Apache-2.0 |
| arrow-ipc | 58.4.1 | Apache-2.0 |
| arrow-schema | 58.4.1 | Apache-2.0 |
| arrow-select | 58.4.1 | Apache-2.0 |
| autocfg | 1.5.1 | Apache-2.0 OR MIT |
| base64 | 0.22.1 | MIT OR Apache-2.0 |
| bitflags | 2.13.2 | MIT OR Apache-2.0 |
| brotli | 8.0.4 | BSD-3-Clause AND MIT |
| brotli-decompressor | 5.0.3 | BSD-3-Clause/MIT |
| bumpalo | 3.20.3 | MIT OR Apache-2.0 |
| byteorder | 1.5.0 | Unlicense OR MIT |
| bytes | 1.12.1 | MIT |
| cc | 1.10.0 | MIT OR Apache-2.0 |
| cfg-if | 1.0.5 | MIT OR Apache-2.0 |
| chrono | 0.4.45 | MIT OR Apache-2.0 |
| const-random | 0.1.18 | MIT OR Apache-2.0 |
| const-random-macro | 0.1.16 | MIT OR Apache-2.0 |
| core-foundation-sys | 0.8.7 | MIT OR Apache-2.0 |
| crunchy | 0.2.4 | MIT |
| find-msvc-tools | 0.1.15 | MIT OR Apache-2.0 |
| flatbuffers | 25.12.19 | Apache-2.0 |
| flate2 | 1.1.10 | MIT OR Apache-2.0 |
| futures-core | 0.3.34 | MIT OR Apache-2.0 |
| futures-task | 0.3.34 | MIT OR Apache-2.0 |
| futures-util | 0.3.34 | MIT OR Apache-2.0 |
| getrandom | 0.2.17 | MIT OR Apache-2.0 |
| getrandom | 0.3.4 | MIT OR Apache-2.0 |
| getrandom | 0.4.3 | MIT OR Apache-2.0 |
| half | 2.7.1 | MIT OR Apache-2.0 |
| hashbrown | 0.17.1 | MIT OR Apache-2.0 |
| iana-time-zone | 0.1.65 | MIT OR Apache-2.0 |
| iana-time-zone-haiku | 0.1.2 | MIT OR Apache-2.0 |
| integer-encoding | 3.0.4 | MIT |
| itoa | 1.0.18 | MIT OR Apache-2.0 |
| jobserver | 0.1.35 | MIT OR Apache-2.0 |
| js-sys | 0.3.106 | MIT OR Apache-2.0 |
| libc | 0.2.191 | MIT OR Apache-2.0 |
| libm | 0.2.16 | MIT |
| log | 0.4.34 | MIT OR Apache-2.0 |
| lz4_flex | 0.13.1 | MIT |
| memchr | 2.8.3 | Unlicense OR MIT |
| num-bigint | 0.4.8 | MIT OR Apache-2.0 |
| num-complex | 0.4.6 | MIT OR Apache-2.0 |
| num-integer | 0.1.47 | MIT OR Apache-2.0 |
| num-traits | 0.2.19 | MIT OR Apache-2.0 |
| once_cell | 1.21.4 | MIT OR Apache-2.0 |
| ordered-float | 3.9.2 | MIT |
| parquet | 58.4.1 | Apache-2.0 |
| paste | 1.0.15 | MIT OR Apache-2.0 |
| pin-project-lite | 0.2.17 | Apache-2.0 OR MIT |
| pkg-config | 0.3.34 | MIT OR Apache-2.0 |
| proc-macro2 | 1.0.107 | MIT OR Apache-2.0 |
| quote | 1.0.47 | MIT OR Apache-2.0 |
| r-efi | 5.3.0 | MIT OR Apache-2.0 OR LGPL-2.1-or-later |
| r-efi | 6.0.0 | MIT OR Apache-2.0 OR LGPL-2.1-or-later |
| rustc_version | 0.4.1 | MIT OR Apache-2.0 |
| rustversion | 1.0.23 | MIT OR Apache-2.0 |
| semver | 1.0.28 | MIT OR Apache-2.0 |
| seq-macro | 0.3.6 | MIT OR Apache-2.0 |
| serde | 1.0.229 | MIT OR Apache-2.0 |
| serde_core | 1.0.229 | MIT OR Apache-2.0 |
| serde_derive | 1.0.229 | MIT OR Apache-2.0 |
| serde_json | 1.0.154 | MIT OR Apache-2.0 |
| shlex | 2.0.1 | MIT OR Apache-2.0 |
| slab | 0.4.12 | MIT |
| snap | 1.1.2 | BSD-3-Clause |
| syn | 2.0.119 | MIT OR Apache-2.0 |
| syn | 3.0.9 | MIT OR Apache-2.0 |
| thiserror | 2.0.21 | MIT OR Apache-2.0 |
| thiserror-impl | 2.0.21 | MIT OR Apache-2.0 |
| thrift | 0.23.0 | Apache-2.0 |
| tiny-keccak | 2.0.2 | CC0-1.0 |
| twox-hash | 2.1.5 | MIT |
| unicode-ident | 1.0.27 | (MIT OR Apache-2.0) AND Unicode-3.0 |
| uuid | 1.18.1 | Apache-2.0 OR MIT |
| version_check | 0.9.5 | MIT/Apache-2.0 |
| wasi | 0.11.1+wasi-snapshot-preview1 | Apache-2.0 WITH LLVM-exception OR Apache-2.0 OR MIT |
| wasip2 | 1.0.4+wasi-0.2.12 | Apache-2.0 WITH LLVM-exception OR Apache-2.0 OR MIT |
| wasm-bindgen | 0.2.129 | MIT OR Apache-2.0 |
| wasm-bindgen-macro | 0.2.129 | MIT OR Apache-2.0 |
| wasm-bindgen-macro-support | 0.2.129 | MIT OR Apache-2.0 |
| wasm-bindgen-shared | 0.2.129 | MIT OR Apache-2.0 |
| windows-core | 0.62.2 | MIT OR Apache-2.0 |
| windows-implement | 0.60.2 | MIT OR Apache-2.0 |
| windows-interface | 0.59.3 | MIT OR Apache-2.0 |
| windows-link | 0.2.1 | MIT OR Apache-2.0 |
| windows-result | 0.4.1 | MIT OR Apache-2.0 |
| windows-strings | 0.5.1 | MIT OR Apache-2.0 |
| wit-bindgen | 0.57.1 | Apache-2.0 WITH LLVM-exception OR Apache-2.0 OR MIT |
| zerocopy | 0.8.63 | BSD-2-Clause OR Apache-2.0 OR MIT |
| zerocopy-derive | 0.8.63 | BSD-2-Clause OR Apache-2.0 OR MIT |
| zlib-rs | 0.6.8 | Zlib |
| zmij | 1.0.23 | MIT |
| zstd | 0.13.3 | MIT |
| zstd-safe | 7.3.0 | BSD-3-Clause |
| zstd-sys | 2.1.2+zstd.1.5.7 | BSD-3-Clause |

The zstd-sys 2.1.2+zstd.1.5.7 source bundles Zstandard 1.5.7 C code under BSD-3-Clause (its alternative COPYING is GPL and is not selected). The active lz4_flex 0.13.1, snap 1.1.2, brotli 8.0.4 and flate2 1.1.10/zlib-rs 0.6.8 backends are Rust implementations; no bundled native code for these backends was identified in their cached packages. The native Zstandard and Unicode notices follow. Distributions must include this file and NOTICE.

### Zstandard BSD notice

```text
BSD License

For Zstandard software

Copyright (c) Meta Platforms, Inc. and affiliates. All rights reserved.

Redistribution and use in source and binary forms, with or without modification,
are permitted provided that the following conditions are met:

 * Redistributions of source code must retain the above copyright notice, this
   list of conditions and the following disclaimer.

 * Redistributions in binary form must reproduce the above copyright notice,
   this list of conditions and the following disclaimer in the documentation
   and/or other materials provided with the distribution.

 * Neither the name Facebook, nor Meta, nor the names of its contributors may
   be used to endorse or promote products derived from this software without
   specific prior written permission.

THIS SOFTWARE IS PROVIDED BY THE COPYRIGHT HOLDERS AND CONTRIBUTORS "AS IS" AND
ANY EXPRESS OR IMPLIED WARRANTIES, INCLUDING, BUT NOT LIMITED TO, THE IMPLIED
WARRANTIES OF MERCHANTABILITY AND FITNESS FOR A PARTICULAR PURPOSE ARE
DISCLAIMED. IN NO EVENT SHALL THE COPYRIGHT HOLDER OR CONTRIBUTORS BE LIABLE FOR
ANY DIRECT, INDIRECT, INCIDENTAL, SPECIAL, EXEMPLARY, OR CONSEQUENTIAL DAMAGES
(INCLUDING, BUT NOT LIMITED TO, PROCUREMENT OF SUBSTITUTE GOODS OR SERVICES;
LOSS OF USE, DATA, OR PROFITS; OR BUSINESS INTERRUPTION) HOWEVER CAUSED AND ON
ANY THEORY OF LIABILITY, WHETHER IN CONTRACT, STRICT LIABILITY, OR TORT
(INCLUDING NEGLIGENCE OR OTHERWISE) ARISING IN ANY WAY OUT OF THE USE OF THIS
SOFTWARE, EVEN IF ADVISED OF THE POSSIBILITY OF SUCH DAMAGE.
```

### Unicode notice

```text
UNICODE LICENSE V3

COPYRIGHT AND PERMISSION NOTICE

Copyright © 1991-2023 Unicode, Inc.

NOTICE TO USER: Carefully read the following legal agreement. BY
DOWNLOADING, INSTALLING, COPYING OR OTHERWISE USING DATA FILES, AND/OR
SOFTWARE, YOU UNEQUIVOCALLY ACCEPT, AND AGREE TO BE BOUND BY, ALL OF THE
TERMS AND CONDITIONS OF THIS AGREEMENT. IF YOU DO NOT AGREE, DO NOT
DOWNLOAD, INSTALL, COPY, DISTRIBUTE OR USE THE DATA FILES OR SOFTWARE.

Permission is hereby granted, free of charge, to any person obtaining a
copy of data files and any associated documentation (the "Data Files") or
software and any associated documentation (the "Software") to deal in the
Data Files or Software without restriction, including without limitation
the rights to use, copy, modify, merge, publish, distribute, and/or sell
copies of the Data Files or Software, and to permit persons to whom the
Data Files or Software are furnished to do so, provided that either (a)
this copyright and permission notice appear with all copies of the Data
Files or Software, or (b) this copyright and permission notice appear in
associated Documentation.

THE DATA FILES AND SOFTWARE ARE PROVIDED "AS IS", WITHOUT WARRANTY OF ANY
KIND, EXPRESS OR IMPLIED, INCLUDING BUT NOT LIMITED TO THE WARRANTIES OF
MERCHANTABILITY, FITNESS FOR A PARTICULAR PURPOSE AND NONINFRINGEMENT OF
THIRD PARTY RIGHTS.

IN NO EVENT SHALL THE COPYRIGHT HOLDER OR HOLDERS INCLUDED IN THIS NOTICE
BE LIABLE FOR ANY CLAIM, OR ANY SPECIAL INDIRECT OR CONSEQUENTIAL DAMAGES,
OR ANY DAMAGES WHATSOEVER RESULTING FROM LOSS OF USE, DATA OR PROFITS,
WHETHER IN AN ACTION OF CONTRACT, NEGLIGENCE OR OTHER TORTIOUS ACTION,
ARISING OUT OF OR IN CONNECTION WITH THE USE OR PERFORMANCE OF THE DATA
FILES OR SOFTWARE.

Except as contained in this notice, the name of a copyright holder shall
not be used in advertising or otherwise to promote the sale, use or other
dealings in these Data Files or Software without prior written
authorization of the copyright holder.
```
