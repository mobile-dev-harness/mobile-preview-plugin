# Third-party notices

MPP is licensed under Apache-2.0; see [LICENSE](LICENSE). Third-party components
retain their own licenses and copyright notices reproduced below and in the
linked files. These notices do not change the licenses of those components.

This inventory covers the macOS Apple Silicon host (`mpp`), Android arm64 native
library (`libmpp_android_device.so`), Java bootstrap, and DSH JavaScript adapter.
It is based on the locked Cargo dependency graphs for `aarch64-apple-darwin` and
`aarch64-linux-android`, Rust 1.88.0, and Android NDK 26.1.10909125. Dependency
presence means inclusion in the target's build/link graph, not proof that every
function survives linker dead-code elimination. The [machine-readable manifest](licenses/third-party/manifest.json)
records target roles, crate checksums, public source locations, and SHA-256 hashes
of all copied notice files. Review this inventory when changing dependencies,
compiler, NDK, or distributed target platforms.

## Runtime dependency graph

Direct third-party Rust dependencies are `serde`, `serde_json`, `thiserror`,
`tokio`, and (Android only) `jni`. `mpp-core`, `mpp-android`, `mpp-host`, and
`mpp-android-device` are MPP workspace code. The table includes transitive runtime
dependencies from both distributed binaries. `simd_cesu8` and `simdutf8` also run
in build-time procedural macros; their runtime role is retained here.

| Crate | Version | License expression | License text |
| --- | --- | --- | --- |
| bytes | 1.12.1 | MIT | [LICENSE](licenses/third-party/crates/bytes-1.12.1/LICENSE) |
| cfg-if | 1.0.5 | MIT OR Apache-2.0 | [LICENSE-APACHE](licenses/third-party/crates/cfg-if-1.0.5/LICENSE-APACHE), [LICENSE-MIT](licenses/third-party/crates/cfg-if-1.0.5/LICENSE-MIT) |
| combine | 4.6.8 | MIT | [LICENSE](licenses/third-party/crates/combine-4.6.8/LICENSE) |
| errno | 0.3.14 | MIT OR Apache-2.0 | [LICENSE-APACHE](licenses/third-party/crates/errno-0.3.14/LICENSE-APACHE), [LICENSE-MIT](licenses/third-party/crates/errno-0.3.14/LICENSE-MIT) |
| itoa | 1.0.18 | MIT OR Apache-2.0 | [LICENSE-APACHE](licenses/third-party/crates/itoa-1.0.18/LICENSE-APACHE), [LICENSE-MIT](licenses/third-party/crates/itoa-1.0.18/LICENSE-MIT) |
| jni | 0.22.4 | MIT OR Apache-2.0 | [LICENSE-APACHE](licenses/third-party/crates/jni-0.22.4/LICENSE-APACHE), [LICENSE-MIT](licenses/third-party/crates/jni-0.22.4/LICENSE-MIT) |
| jni-sys | 0.4.1 | MIT OR Apache-2.0 | [LICENSE-APACHE](licenses/third-party/crates/jni-sys-0.4.1/LICENSE-APACHE), [LICENSE-MIT](licenses/third-party/crates/jni-sys-0.4.1/LICENSE-MIT) |
| libc | 0.2.190 | MIT OR Apache-2.0 | [LICENSE-APACHE](licenses/third-party/crates/libc-0.2.190/LICENSE-APACHE), [LICENSE-MIT](licenses/third-party/crates/libc-0.2.190/LICENSE-MIT) |
| log | 0.4.34 | MIT OR Apache-2.0 | [LICENSE-APACHE](licenses/third-party/crates/log-0.4.34/LICENSE-APACHE), [LICENSE-MIT](licenses/third-party/crates/log-0.4.34/LICENSE-MIT) |
| memchr | 2.8.3 | Unlicense OR MIT | [COPYING](licenses/third-party/crates/memchr-2.8.3/COPYING), [LICENSE-MIT](licenses/third-party/crates/memchr-2.8.3/LICENSE-MIT), [UNLICENSE](licenses/third-party/crates/memchr-2.8.3/UNLICENSE) |
| mio | 1.2.4 | MIT | [LICENSE](licenses/third-party/crates/mio-1.2.4/LICENSE) |
| pin-project-lite | 0.2.17 | Apache-2.0 OR MIT | [LICENSE-APACHE](licenses/third-party/crates/pin-project-lite-0.2.17/LICENSE-APACHE), [LICENSE-MIT](licenses/third-party/crates/pin-project-lite-0.2.17/LICENSE-MIT) |
| serde | 1.0.229 | MIT OR Apache-2.0 | [LICENSE-APACHE](licenses/third-party/crates/serde-1.0.229/LICENSE-APACHE), [LICENSE-MIT](licenses/third-party/crates/serde-1.0.229/LICENSE-MIT) |
| serde_core | 1.0.229 | MIT OR Apache-2.0 | [LICENSE-APACHE](licenses/third-party/crates/serde_core-1.0.229/LICENSE-APACHE), [LICENSE-MIT](licenses/third-party/crates/serde_core-1.0.229/LICENSE-MIT) |
| serde_json | 1.0.151 | MIT OR Apache-2.0 | [LICENSE-APACHE](licenses/third-party/crates/serde_json-1.0.151/LICENSE-APACHE), [LICENSE-MIT](licenses/third-party/crates/serde_json-1.0.151/LICENSE-MIT) |
| signal-hook-registry | 1.4.8 | MIT OR Apache-2.0 | [LICENSE-APACHE](licenses/third-party/crates/signal-hook-registry-1.4.8/LICENSE-APACHE), [LICENSE-MIT](licenses/third-party/crates/signal-hook-registry-1.4.8/LICENSE-MIT) |
| simd_cesu8 | 1.2.0 | Apache-2.0 OR MIT | [LICENSE-APACHE](licenses/third-party/crates/simd_cesu8-1.2.0/LICENSE-APACHE), [LICENSE-MIT](licenses/third-party/crates/simd_cesu8-1.2.0/LICENSE-MIT) |
| simdutf8 | 0.1.5 | MIT OR Apache-2.0 | [LICENSE-Apache](licenses/third-party/crates/simdutf8-0.1.5/LICENSE-Apache), [LICENSE-MIT](licenses/third-party/crates/simdutf8-0.1.5/LICENSE-MIT) |
| socket2 | 0.6.5 | MIT OR Apache-2.0 | [LICENSE-APACHE](licenses/third-party/crates/socket2-0.6.5/LICENSE-APACHE), [LICENSE-MIT](licenses/third-party/crates/socket2-0.6.5/LICENSE-MIT) |
| thiserror | 2.0.21 | MIT OR Apache-2.0 | [LICENSE-APACHE](licenses/third-party/crates/thiserror-2.0.21/LICENSE-APACHE), [LICENSE-MIT](licenses/third-party/crates/thiserror-2.0.21/LICENSE-MIT) |
| tokio | 1.53.2 | MIT | [LICENSE](licenses/third-party/crates/tokio-1.53.2/LICENSE) |
| zmij | 1.0.23 | MIT | [LICENSE-MIT](licenses/third-party/crates/zmij-1.0.23/LICENSE-MIT) |

## Build-time dependencies

These crates execute as procedural macros or build-script dependencies. They are
not standalone runtime libraries in the distributed package. Their notices are
also retained to make the build provenance and generated-code sources explicit.
`serde_json` is the only workspace dev-dependency; it is already covered as a
runtime dependency. No additional test-only crates are distributed.

| Crate | Version | License expression | License text |
| --- | --- | --- | --- |
| jni-macros | 0.22.4 | MIT OR Apache-2.0 | [LICENSE-APACHE](licenses/third-party/crates/jni-macros-0.22.4/LICENSE-APACHE), [LICENSE-MIT](licenses/third-party/crates/jni-macros-0.22.4/LICENSE-MIT) |
| jni-sys-macros | 0.4.1 | MIT OR Apache-2.0 | [LICENSE-APACHE](licenses/third-party/crates/jni-sys-macros-0.4.1/LICENSE-APACHE), [LICENSE-MIT](licenses/third-party/crates/jni-sys-macros-0.4.1/LICENSE-MIT) |
| proc-macro2 | 1.0.107 | MIT OR Apache-2.0 | [LICENSE-APACHE](licenses/third-party/crates/proc-macro2-1.0.107/LICENSE-APACHE), [LICENSE-MIT](licenses/third-party/crates/proc-macro2-1.0.107/LICENSE-MIT) |
| quote | 1.0.47 | MIT OR Apache-2.0 | [LICENSE-APACHE](licenses/third-party/crates/quote-1.0.47/LICENSE-APACHE), [LICENSE-MIT](licenses/third-party/crates/quote-1.0.47/LICENSE-MIT) |
| rustc_version | 0.4.1 | MIT OR Apache-2.0 | [LICENSE-APACHE](licenses/third-party/crates/rustc_version-0.4.1/LICENSE-APACHE), [LICENSE-MIT](licenses/third-party/crates/rustc_version-0.4.1/LICENSE-MIT) |
| same-file | 1.0.6 | Unlicense/MIT | [COPYING](licenses/third-party/crates/same-file-1.0.6/COPYING), [LICENSE-MIT](licenses/third-party/crates/same-file-1.0.6/LICENSE-MIT), [UNLICENSE](licenses/third-party/crates/same-file-1.0.6/UNLICENSE) |
| semver | 1.0.28 | MIT OR Apache-2.0 | [LICENSE-APACHE](licenses/third-party/crates/semver-1.0.28/LICENSE-APACHE), [LICENSE-MIT](licenses/third-party/crates/semver-1.0.28/LICENSE-MIT) |
| serde_derive | 1.0.229 | MIT OR Apache-2.0 | [LICENSE-APACHE](licenses/third-party/crates/serde_derive-1.0.229/LICENSE-APACHE), [LICENSE-MIT](licenses/third-party/crates/serde_derive-1.0.229/LICENSE-MIT) |
| syn | 2.0.119 | MIT OR Apache-2.0 | [LICENSE-APACHE](licenses/third-party/crates/syn-2.0.119/LICENSE-APACHE), [LICENSE-MIT](licenses/third-party/crates/syn-2.0.119/LICENSE-MIT) |
| syn | 3.0.6 | MIT OR Apache-2.0 | [LICENSE-APACHE](licenses/third-party/crates/syn-3.0.6/LICENSE-APACHE), [LICENSE-MIT](licenses/third-party/crates/syn-3.0.6/LICENSE-MIT) |
| thiserror-impl | 2.0.21 | MIT OR Apache-2.0 | [LICENSE-APACHE](licenses/third-party/crates/thiserror-impl-2.0.21/LICENSE-APACHE), [LICENSE-MIT](licenses/third-party/crates/thiserror-impl-2.0.21/LICENSE-MIT) |
| tokio-macros | 2.7.2 | MIT | [LICENSE](licenses/third-party/crates/tokio-macros-2.7.2/LICENSE) |
| unicode-ident | 1.0.26 | (MIT OR Apache-2.0) AND Unicode-3.0 | [LICENSE-APACHE](licenses/third-party/crates/unicode-ident-1.0.26/LICENSE-APACHE), [LICENSE-MIT](licenses/third-party/crates/unicode-ident-1.0.26/LICENSE-MIT), [LICENSE-UNICODE](licenses/third-party/crates/unicode-ident-1.0.26/LICENSE-UNICODE) |
| walkdir | 2.5.0 | Unlicense/MIT | [COPYING](licenses/third-party/crates/walkdir-2.5.0/COPYING), [LICENSE-MIT](licenses/third-party/crates/walkdir-2.5.0/LICENSE-MIT), [UNLICENSE](licenses/third-party/crates/walkdir-2.5.0/UNLICENSE) |

`jni`, `jni-macros`, and `jni-sys-macros` declare their licenses in published Cargo
metadata but omit the license files from their crate archives. Their copies here
come from the exact source commits recorded in each crate's `.cargo_vcs_info.json`.
Those commits contain `LICENSE-APACHE` and `LICENSE-MIT`, with no additional
`NOTICE` file. Source commits and hashes are in the manifest.

## Rust standard library and native support

The binaries also contain Rust standard-library code, which is not listed in
MPP's Cargo.lock. Its notices are supplied as:

- [Rust 1.88.0 LICENSE-APACHE](licenses/third-party/rust-1.88.0/LICENSE-APACHE)
- [Rust 1.88.0 LICENSE-MIT](licenses/third-party/rust-1.88.0/LICENSE-MIT)
- [Rust 1.88.0 COPYRIGHT](licenses/third-party/rust-1.88.0/COPYRIGHT)
- [Rust 1.88.0 standard-library component notices](licenses/third-party/rust-1.88.0/COPYRIGHT-library.html)

The standard-library component notices include the upstream licenses and
attributions for its in-tree code and third-party dependencies. They are kept
verbatim instead of treating all standard-library code as one license.

The Android library contains static unwind/compiler support. To preserve the
applicable NDK and LLVM runtime notices, this package also carries the unmodified
[NDK r26b NOTICE](licenses/third-party/android-ndk-26.1.10909125/NOTICE) and
[LLVM NOTICE](licenses/third-party/android-ndk-26.1.10909125/LLVM-NOTICE).
These upstream compilations cover more components than this package uses; their
inclusion does not mean MPP distributes the NDK, compiler, or all listed libraries.

## External platform components

The DSH adapter has no bundled third-party Node packages. It imports Node's
built-in modules and uses the React, store, and UI modules supplied by the user's
DSH installation. Node, React, DSH, adb, Android framework classes, and Android
media codecs are not included in this package. The Java bootstrap is MPP source;
its compiled archive does not include Android SDK classes.

The macOS host dynamically links the operating system's `libSystem.B.dylib` and
`libiconv.2.dylib`. The Android library dynamically links the device's
`libmediandk.so`, `libandroid.so`, `libdl.so`, and `libc.so`. Those operating-system
libraries are not redistributed in the preview archive.

## Design references

MPP's video/control separation was informed by research of
[scrcpy v4.1](https://github.com/Genymobile/scrcpy/tree/2926c06c5dc3064ae6d8db706f1a98a37cfcf3f0).
MPP does not bundle or run scrcpy and does not implement scrcpy's wire protocol.
The repository documents the research basis in `docs/DESIGN.md`; the dependency
and package inventories contain no scrcpy component. This attribution identifies
the design reference and is not a statement that scrcpy is distributed in MPP.
