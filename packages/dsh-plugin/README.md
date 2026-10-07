# Mobile Preview Plugin for DSH

An **unofficial development preview** for viewing and controlling an Android
device inside DeepSeek Harness Web and Desktop. MPP is independent of
mobile-dev-harness (mdh). It provides a device panel, H.264 video, single-pointer
input and basic Android keys; it does not expose agent tools or give a model vision.

## Prerequisites

- An Apple Silicon DSH Host running macOS 14 or later. This archive bundles only a `darwin-arm64` host.
- Android SDK platform-tools (`adb`) and an authorized **Android API 29–37
  arm64-v8a device** attached to that Host. Emulator use also requires emulator
  tools and an existing AVD. This is an implemented adapter range, not a completed
  compatibility matrix. Non-arm64 devices and iOS are outside the live backend.
- An H.264 WebCodecs-capable client. Earlier preview archives were checked on the
  official npm DSH `0.2.1-alpha.1` Web distribution. Source Web/Desktop checks used
  commit `5badb15009ae1756c3afe0ae0cef1faafc290ccc`. The official Desktop
  `0.2.0-rc.2` passed CLI installation and GUI checks on API 37 using the exact
  downloaded CI preview.3 archive identified below.

Rust, Android NDK and JDK are build prerequisites only; they are not needed to use
the archive. The bundled Rust host and Android assets are resolved relative to the
installed package. No `MPP_EXECUTABLE` environment variable is required. SDK tools
are located through `ANDROID_HOME`, `ANDROID_SDK_ROOT`, conventional SDK locations
and `PATH`.

## Install and connect

In DSH, open **Plugins → Add plugin**, enter the absolute path to this `.tgz`,
install it and enable it. For a Web CLI profile:

```sh
dsh plugin --profile web add /absolute/path/mobile-dev-harness-dsh-mobile-preview-0.1.0-preview.4-darwin-arm64.tgz
```

`0.1.0-preview.4` is an example development version, not a published or qualified
release. Substitute the archive version and path you actually built.

After installing a new native package version, restart the DSH Host; hot reload can
retain an old package-relative native path. Then open a conversation's mobile-device
panel, select the exact authorized device in the implemented API/ABI range
and choose **Connect**. Starting an existing AVD is an explicit action. Preview
starts automatically when the panel is visible; use **Pause** and **Resume** to
control it. Click or drag within the screen, or use Home/Back. A focused screen
also accepts basic navigation keys. An expired device connection requires a new
connection. Disconnecting does not shut down the emulator.

No DSH source patch is required for video or input. Stock DSH uses manual sidebar
width; automatic phone-width fitting is an optional source extension. Web shows
devices attached to the DSH Host, not automatically the browser's machine.

The earlier `0.1.0-preview.1` archive passed clean installation on stock DSH Web
with the API 32 emulator in an isolated profile without source patches,
runtime-path overrides or a model API
key. Checks covered automatic first frame, tap/drag, Home/Back, manual sidebar
resizing, Pause/Resume across panel reopening, and removal while connected. That
removal unloaded the UI and cleared the MPP host, device bootstrap process and adb
reverse mappings. Earlier Desktop GUI checks used a source build.

The official Desktop `0.2.0-rc.2` app passed signature and notarization checks.
Its bundled CLI installed `0.1.0-preview.3` into an isolated profile with the bundle
enabled and native hashes matching the archive. The isolated profile used a
separate webserver port to avoid the existing app instance. This installation
check is separate from the later GUI qualification of the downloaded CI archive.

The downloaded `0.1.0-preview.3` from workflow run `37652914514`, source commit
`9ec7afad96f486b8050f2f41e6ca8bade24c9b8c`, has SHA-256
`d6f6450826638bdb44cce5a2b8669965036f7de87ec8d79e2468d3b5d08f9b9c`.
On official Desktop `0.2.0-rc.2` and the API 37 emulator, it passed decoded first
frame, Settings/Apps taps, dragging, Back/Home, Pause across panel reopening,
Resume to Live and a native window-zoom round trip remaining Live. GUI removal
removed its entry and renderer, dependency/bundle and owned helper/reverse mapping;
GUI reinstallation and enabling restored Live. Plugin Manager hides the preview,
so removal began from a previously connected chat.

On stock Web `0.2.1-alpha.1`, the same CI archive passed decoded first frame,
dragging, Settings/Apps taps and Back/Home on API 35–37. API 37 also passed
Pause/Resume across panel reopening and a 1280×820/narrow-viewport round trip
remaining Live. GUI removal after the API 35 connected session removed its entry,
replaced the device view with DSH's no-renderer placeholder, cleared the dependency
and bundle, and left no helper or owned reverse mapping. The panel was hidden
before entering Plugin Manager. GUI reinstallation, enabling and reconnecting on
API 35 restored a decoded Live view. Records and screenshots are under
`target/release-readiness/gui/` in the source checkout.

The earlier `0.1.0-preview.2` archive was installed into the official stock npm DSH
`0.2.1-alpha.1` Web distribution on macOS arm64. On one nubia P0110 physical phone
(Android 16/API 36, arm64), it decoded a live 576×1280 stream from the 1264×2800
display. Home/Back, a canvas tap into Display & Brightness, settings-list dragging,
and Pause/Resume had visible effects. Pause left no helper process. Three separate
native start/capture/stop cycles cleaned up helper processes, adb reverse mappings
and MPP virtual displays. The same host/device assets also passed an API 32 emulator
capture/cleanup regression.

These are historical passes for those packages and targets; they do not qualify
the preview.3 matrix candidate on every API or vendor. Its native testing has
passed H.264, Home effects and cleanup on the API 30–37 matrix targets; stock DSH
Web first-frame and input checks passed on API 30–34. The later API 35–37 Web and
official Desktop GUI passes use the CI archive identified above, not this local
matrix archive. Its same version number does not transfer qualification between
the two checksums. The API 37 native run also passed with an observed 16,384-byte
page size.
The tested API 29 emulator environment is blocked by a Codec2 surface failure that
also affected scrcpy 4.1 and system screenrecord. The final host-GPU/Vulkan-disabled
control still failed with no video packets, while cleanup passed. No workaround is
shipped. See the source repository's
`docs/ANDROID-COMPATIBILITY.md` for exact target evidence.

Physical-device rotation, hot unplug and long GUI sessions were not tested in this
increment. The CI archive's GUI checks used emulators and do not extend the earlier
phone's coverage to other vendors, physical USB hot unplug, sustained sessions or
extreme interruptions. Audio,
recording, multi-touch, text/IME and clipboard integration are not implemented.
Device rotation ends the current capture epoch.

## Remove

Disconnect the device, then remove the plugin through DSH's plugin manager. For
the same Web profile used during installation:

```sh
dsh plugin --profile web remove @mobile-dev-harness/dsh-mobile-preview
```

The emulator remains running and can be shut down separately when no longer needed.

## Build and integrity

The source repository's `node scripts/package-preview.mjs --version 0.1.0-preview.4`
builds a local preview archive and adjacent SHA-256 checksum. An explicit version
matching `<source-version>-preview.<number>` is required; existing archives are not
overwritten. Packaging requires the installed Rust 1.88.0 and Android NDK
26.1.10909125 versions recorded by the third-party license inventory.
`runtime-manifest.json` records the source commit, dirty state, source-tree SHA-256,
build targets, versions and hashes for `native/darwin-arm64/mpp` and the bundled
Android bootstrap and native library under `native/android-arm64`. The manifest records `minApi: 29`
and implemented `supportedApis` 29–37, not a list of passed device tests. ELF LOAD
segments are checked for 16 KiB alignment. The separate API 37 matrix run verifies
native behavior on one 16 KiB-page emulator; it is not a blanket device guarantee.
These integrity records are not signatures. Explicit absolute `executable` and
`deviceAssets` configuration remains supported for development.

MPP is licensed under Apache-2.0. The archive includes `LICENSE`,
`THIRD-PARTY-NOTICES.md` and `licenses/third-party/`; third-party components retain
their own licenses. Packaging checks the inventory against `Cargo.lock` and the
notice-file hashes, and rejects source changes during the build.

The source repository remains private; source and staged package manifests retain
`private: true`. This is a development preview, not a 1.0 stable release. Local
packaging does not publish to npm or GitHub Releases. Neither has been authorized
or performed; the source repository's `docs/RELEASING.md` tracks the tested scope,
remaining limits and publication review. MPP is not an official DeepSeek plugin or
distribution component.
