#!/usr/bin/env bash
set -euo pipefail

fail() {
  printf 'build-android-device: %s\n' "$*" >&2
  exit 2
}

[[ $# -eq 0 ]] || fail 'This script takes no arguments; configure CARGO, RUSTUP_TOOLCHAIN, ANDROID_NDK_HOME, ANDROID_HOME, JAVA_HOME, MPP_BUILD_TOOLS, MPP_ANDROID_JAR, or MPP_BUILD_PROFILE instead.'

MHP_PROFILE="${MPP_BUILD_PROFILE:-debug}"
case "$MHP_PROFILE" in
  debug|release) ;;
  *) fail 'MPP_BUILD_PROFILE must be debug or release.' ;;
esac

MHP_ROOT="$(cd -- "$(dirname "${BASH_SOURCE[0]}")/.." >/dev/null && pwd -P)"
MHP_TARGET=aarch64-linux-android
MHP_API=29
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
MHP_READELF="$MHP_NDK/toolchains/llvm/prebuilt/$MHP_PREBUILT/bin/llvm-readelf"
[[ -x "$MHP_LINKER" ]] || fail "NDK compiler is unavailable: $MHP_LINKER"
[[ -x "$MHP_READELF" ]] || fail "NDK ELF inspector is unavailable: $MHP_READELF"
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

MHP_BOOTSTRAP_SOURCE="$MHP_ROOT/android-bootstrap/dev/mpp/Bootstrap.java"
[[ -f "$MHP_BOOTSTRAP_SOURCE" ]] || fail "Bootstrap source is unavailable: $MHP_BOOTSTRAP_SOURCE"
if [[ -n "${JAVA_HOME:-}" ]]; then
  MHP_JAVAC="$JAVA_HOME/bin/javac"
  MHP_JAR="$JAVA_HOME/bin/jar"
else
  MHP_JAVAC="$(command -v javac || true)"
  MHP_JAR="$(command -v jar || true)"
fi
[[ -n "$MHP_JAVAC" && -x "$MHP_JAVAC" && -n "$MHP_JAR" && -x "$MHP_JAR" ]] || fail 'An installed JDK 17 or later is required. Set JAVA_HOME to its root or put javac and jar on PATH; nothing is installed automatically.'
MHP_JAVAC_VERSION="$("$MHP_JAVAC" -version 2>&1)" || fail 'javac cannot run. Set JAVA_HOME to an installed JDK 17 or later.'
[[ "$MHP_JAVAC_VERSION" =~ javac[[:space:]]+([0-9]+) ]] || fail 'Could not identify the javac version; select an installed JDK 17 or later.'
[[ "${BASH_REMATCH[1]}" -ge 17 ]] || fail 'JDK 17 or later is required to build the bootstrap with --release 8.'

MHP_BUILD_TOOLS="${MPP_BUILD_TOOLS:-}"
if [[ -z "$MHP_BUILD_TOOLS" ]]; then
  while IFS= read -r MHP_VERSION; do
    MHP_CANDIDATE="$MHP_SDK/build-tools/$MHP_VERSION"
    if [[ -x "$MHP_CANDIDATE/d8" ]]; then
      MHP_BUILD_TOOLS="$MHP_CANDIDATE"
      break
    fi
  done < <(
    for MHP_CANDIDATE in "$MHP_SDK"/build-tools/*; do
      [[ -d "$MHP_CANDIDATE" ]] || continue
      MHP_VERSION="${MHP_CANDIDATE##*/}"
      [[ "$MHP_VERSION" =~ ^[0-9]+\.[0-9]+\.[0-9]+$ ]] || continue
      printf '%s\n' "$MHP_VERSION"
    done | sort -t . -k1,1nr -k2,2nr -k3,3nr
  )
fi
[[ -n "$MHP_BUILD_TOOLS" && "$MHP_BUILD_TOOLS" = /* && -x "$MHP_BUILD_TOOLS/d8" ]] || fail 'Installed Android build-tools with d8 are required. Set MPP_BUILD_TOOLS to an absolute build-tools version directory; nothing is downloaded automatically.'
MHP_D8="$MHP_BUILD_TOOLS/d8"
"$MHP_D8" --version >/dev/null 2>&1 || fail 'd8 cannot run; check the installed build-tools and JAVA_HOME.'

MHP_ANDROID_JAR="${MPP_ANDROID_JAR:-}"
if [[ -z "$MHP_ANDROID_JAR" ]]; then
  while IFS= read -r MHP_PLATFORM; do
    MHP_CANDIDATE="$MHP_SDK/platforms/android-$MHP_PLATFORM/android.jar"
    if [[ -f "$MHP_CANDIDATE" ]]; then
      MHP_ANDROID_JAR="$MHP_CANDIDATE"
      break
    fi
  done < <(
    for MHP_CANDIDATE in "$MHP_SDK"/platforms/android-*; do
      [[ -d "$MHP_CANDIDATE" ]] || continue
      MHP_PLATFORM="${MHP_CANDIDATE##*/android-}"
      [[ "$MHP_PLATFORM" =~ ^[0-9]+$ ]] || continue
      [[ "$MHP_PLATFORM" -ge "$MHP_API" ]] || continue
      printf '%s\n' "$MHP_PLATFORM"
    done | sort -n
  )
fi
[[ -n "$MHP_ANDROID_JAR" && "$MHP_ANDROID_JAR" = /* && -f "$MHP_ANDROID_JAR" && -r "$MHP_ANDROID_JAR" ]] || fail 'An installed Android platform android.jar (API 29 or later) is required. Set MPP_ANDROID_JAR to its absolute path; no SDK packages are installed automatically.'

MHP_ARGS=(
  build --locked
  --manifest-path "$MHP_ROOT/Cargo.toml"
  --target-dir "$MHP_TARGET_DIR"
  --package mpp-android-device
  --target "$MHP_TARGET"
  --lib --example codec_probe
)
if [[ "$MHP_PROFILE" = release ]]; then
  MHP_ARGS+=(--release)
fi

mkdir -p "$MHP_TARGET_DIR"
MHP_STAGE="$(mktemp -d "$MHP_TARGET_DIR/.mpp-android-build.XXXXXX")"
trap 'rm -rf -- "$MHP_STAGE"' EXIT
# Scope alignment to the Android linker, preserving Cargo's caller/config flag precedence.
# NDK r27 and earlier require both flags: developer.android.com/guide/practices/page-sizes.
cat > "$MHP_STAGE/android-linker" <<'LINKER'
#!/usr/bin/env bash
set -euo pipefail
exec "${MPP_ANDROID_LINKER:?}" "$@" -Wl,-z,max-page-size=16384 -Wl,-z,common-page-size=16384
LINKER
chmod +x "$MHP_STAGE/android-linker"

printf 'Building Android API %s for %s (%s) with NDK %s\n' "$MHP_API" "$MHP_TARGET" "$MHP_PROFILE" "$MHP_NDK" >&2
RUSTUP_AUTO_INSTALL=0 MPP_ANDROID_LINKER="$MHP_LINKER" \
  CARGO_TARGET_AARCH64_LINUX_ANDROID_LINKER="$MHP_STAGE/android-linker" \
  "$MHP_CARGO" "${MHP_ARGS[@]}"

MHP_LIBRARY="$MHP_TARGET_DIR/$MHP_TARGET/$MHP_PROFILE/libmpp_android_device.so"
MHP_PROBE="$MHP_TARGET_DIR/$MHP_TARGET/$MHP_PROFILE/examples/codec_probe"
[[ -f "$MHP_LIBRARY" ]] || fail "Cargo did not produce the expected shared library: $MHP_LIBRARY"
[[ -f "$MHP_PROBE" ]] || fail "Cargo did not produce the expected codec probe: $MHP_PROBE"

MHP_HEADERS="$("$MHP_READELF" -W -l "$MHP_LIBRARY")" || fail 'Cannot inspect the native library ELF load segments.'
MHP_LOAD_COUNT=0
while IFS= read -r MHP_LINE; do
  read -r -a MHP_COLUMNS <<< "$MHP_LINE"
  [[ "${MHP_COLUMNS[0]:-}" = LOAD ]] || continue
  MHP_ALIGN="${MHP_COLUMNS[${#MHP_COLUMNS[@]}-1]}"
  [[ "$MHP_ALIGN" =~ ^0x[0-9a-fA-F]{1,8}$ ]] || fail "Invalid ELF load alignment: $MHP_ALIGN"
  [[ "$((MHP_ALIGN))" -ge 16384 ]] || fail "Native library LOAD alignment $MHP_ALIGN is below 16 KiB; check linker overrides. Existing assets were not replaced."
  MHP_LOAD_COUNT=$((MHP_LOAD_COUNT + 1))
done <<< "$MHP_HEADERS"
[[ "$MHP_LOAD_COUNT" -gt 0 ]] || fail 'Native library has no ELF LOAD segments; existing assets were not replaced.'

mkdir "$MHP_STAGE/classes"
printf 'Compiling Java bootstrap with %s and %s (minimum API %s)\n' "$MHP_JAVAC_VERSION" "$MHP_ANDROID_JAR" "$MHP_API" >&2
"$MHP_JAVAC" --release 8 -encoding UTF-8 -classpath "$MHP_ANDROID_JAR" \
  -d "$MHP_STAGE/classes" "$MHP_BOOTSTRAP_SOURCE"
MHP_CLASSES=()
while IFS= read -r -d '' MHP_CLASS; do
  MHP_CLASSES+=("$MHP_CLASS")
done < <(find "$MHP_STAGE/classes" -type f -name '*.class' -print0)
[[ "${#MHP_CLASSES[@]}" -gt 0 ]] || fail 'javac produced no bootstrap classes.'
[[ -f "$MHP_STAGE/classes/dev/mpp/Bootstrap.class" ]] || fail 'javac did not produce dev.mpp.Bootstrap.'
"$MHP_D8" --release --min-api "$MHP_API" --lib "$MHP_ANDROID_JAR" \
  --output "$MHP_STAGE/bootstrap.jar" "${MHP_CLASSES[@]}"
[[ -f "$MHP_STAGE/bootstrap.jar" ]] || fail 'd8 did not produce bootstrap.jar.'
MHP_JAR_ENTRIES="$("$MHP_JAR" tf "$MHP_STAGE/bootstrap.jar")" || fail 'd8 output is not a readable JAR archive.'
[[ $'\n'"$MHP_JAR_ENTRIES"$'\n' = *$'\nclasses.dex\n'* ]] || fail 'The bootstrap JAR has no classes.dex entry.'

cp "$MHP_LIBRARY" "$MHP_STAGE/libmpp_android_device.so"
MHP_ASSETS="$MHP_TARGET_DIR/android-device"
mkdir -p "$MHP_ASSETS"
mv -f "$MHP_STAGE/bootstrap.jar" "$MHP_ASSETS/bootstrap.jar"
mv -f "$MHP_STAGE/libmpp_android_device.so" "$MHP_ASSETS/libmpp_android_device.so"
printf '%s\n' "$MHP_ASSETS/bootstrap.jar" "$MHP_ASSETS/libmpp_android_device.so" "$MHP_PROBE"
