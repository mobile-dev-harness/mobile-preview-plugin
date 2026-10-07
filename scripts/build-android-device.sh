#!/usr/bin/env bash
set -euo pipefail

fail() {
  printf 'build-android-device: %s\n' "$*" >&2
  exit 2
}

[[ $# -eq 0 ]] || fail 'This script takes no arguments; configure CARGO, RUSTUP_TOOLCHAIN, ANDROID_NDK_HOME, or ANDROID_HOME instead.'

MHP_ROOT="$(cd -- "$(dirname "${BASH_SOURCE[0]}")/.." >/dev/null && pwd -P)"
MHP_TARGET=aarch64-linux-android
MHP_API=32
MHP_CARGO="${CARGO:-cargo}"
MHP_RUSTC="${RUSTC:-rustc}"
MHP_TARGET_DIR="${CARGO_TARGET_DIR:-$MHP_ROOT/target}"
[[ "$MHP_TARGET_DIR" = /* ]] || MHP_TARGET_DIR="$MHP_ROOT/$MHP_TARGET_DIR"

command -v "$MHP_CARGO" >/dev/null 2>&1 || fail "Cargo executable is unavailable: $MHP_CARGO"
command -v "$MHP_RUSTC" >/dev/null 2>&1 || fail "Rust compiler is unavailable: $MHP_RUSTC"

case "$(uname -s)" in
  Darwin)
    MHP_PREBUILT=darwin-x86_64
    MHP_DEFAULT_SDK="${HOME:?HOME is required}/Library/Android/sdk"
    ;;
  Linux)
    [[ "$(uname -m)" = x86_64 ]] || fail 'This prototype requires the Linux x86_64 or macOS NDK host tools.'
    MHP_PREBUILT=linux-x86_64
    MHP_DEFAULT_SDK="${HOME:?HOME is required}/Android/Sdk"
    ;;
  *) fail 'This prototype build supports macOS and Linux hosts only.' ;;
esac

MHP_SDK="${ANDROID_HOME:-${ANDROID_SDK_ROOT:-$MHP_DEFAULT_SDK}}"
MHP_NDK="${ANDROID_NDK_HOME:-${ANDROID_NDK_LATEST_HOME:-}}"
if [[ -z "$MHP_NDK" ]]; then
  while IFS= read -r MHP_VERSION; do
    MHP_CANDIDATE="$MHP_SDK/ndk/$MHP_VERSION"
    if [[ -x "$MHP_CANDIDATE/toolchains/llvm/prebuilt/$MHP_PREBUILT/bin/aarch64-linux-android$MHP_API-clang" ]]; then
      MHP_NDK="$MHP_CANDIDATE"
      break
    fi
  done < <(
    for MHP_CANDIDATE in "$MHP_SDK"/ndk/*; do
      [[ -d "$MHP_CANDIDATE" ]] || continue
      MHP_VERSION="${MHP_CANDIDATE##*/}"
      [[ "$MHP_VERSION" =~ ^[0-9]+\.[0-9]+\.[0-9]+$ ]] || continue
      printf '%s\n' "$MHP_VERSION"
    done | sort -t . -k1,1nr -k2,2nr -k3,3nr
  )
fi
[[ -n "$MHP_NDK" ]] || fail "No installed Android NDK with API $MHP_API tools was found under $MHP_SDK/ndk. Set ANDROID_NDK_HOME to an installed NDK; nothing is downloaded automatically."
[[ -d "$MHP_NDK" ]] || fail "Selected Android NDK path does not name a directory: $MHP_NDK"
MHP_NDK="$(cd -- "$MHP_NDK" >/dev/null && pwd -P)"
MHP_LINKER="$MHP_NDK/toolchains/llvm/prebuilt/$MHP_PREBUILT/bin/aarch64-linux-android$MHP_API-clang"
[[ -x "$MHP_LINKER" ]] || fail "NDK compiler is unavailable: $MHP_LINKER"
"$MHP_LINKER" --version >/dev/null 2>&1 || fail "The NDK compiler cannot run on this host: $MHP_LINKER"

MHP_LIBDIR="$(RUSTUP_AUTO_INSTALL=0 "$MHP_RUSTC" --print target-libdir --target "$MHP_TARGET")" || fail "Cannot inspect the selected Rust compiler; check RUSTC and RUSTUP_TOOLCHAIN."
MHP_HAS_STD=false
for MHP_LIBRARY in "$MHP_LIBDIR"/libstd-*.rlib; do
  if [[ -f "$MHP_LIBRARY" ]]; then
    MHP_HAS_STD=true
    break
  fi
done
[[ "$MHP_HAS_STD" = true ]] || fail "Rust target $MHP_TARGET is not installed for the selected compiler. Install it explicitly with rustup target add $MHP_TARGET (using the same RUSTUP_TOOLCHAIN), then retry."

MHP_ARGS=(
  build --locked
  --manifest-path "$MHP_ROOT/Cargo.toml"
  --target-dir "$MHP_TARGET_DIR"
  --package mpp-android-device
  --target "$MHP_TARGET"
  --lib --example codec_probe
)

printf 'Building Android API %s for %s with NDK %s\n' "$MHP_API" "$MHP_TARGET" "$MHP_NDK" >&2
RUSTUP_AUTO_INSTALL=0 CARGO_TARGET_AARCH64_LINUX_ANDROID_LINKER="$MHP_LINKER" \
  "$MHP_CARGO" "${MHP_ARGS[@]}"

MHP_LIBRARY="$MHP_TARGET_DIR/$MHP_TARGET/debug/libmpp_android_device.so"
MHP_PROBE="$MHP_TARGET_DIR/$MHP_TARGET/debug/examples/codec_probe"
[[ -f "$MHP_LIBRARY" ]] || fail "Cargo did not produce the expected shared library: $MHP_LIBRARY"
[[ -f "$MHP_PROBE" ]] || fail "Cargo did not produce the expected codec probe: $MHP_PROBE"
printf '%s\n' "$MHP_LIBRARY" "$MHP_PROBE"
