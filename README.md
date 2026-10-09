# Mobile Preview Plugin

[Technical overview and usage guide](https://mobile-dev-harness.github.io/mobile-preview-plugin-site/)

[简体中文](README.zh-CN.md)

Mobile Preview Plugin (MPP) is an independent Rust framework for connecting mobile
devices to DeepSeek Harness Web and Desktop. The intended product provides live
preview and input for emulators, simulators and physical devices, without requiring
mobile-dev-harness (mdh).

The public repository is preparing its first GitHub Release, **`0.1.0-preview.5`**:
existing Android support plus local iOS Simulator support. Final CI artifact
verification and publication are pending; see the [release checklist](docs/RELEASING.md).
The npm package remains private. This is a development preview, not stable 1.0.

**Current status: development preview with Android video/control and experimental
capability-gated iOS Simulator video, single-pointer tap/drag and Home.** The
current local source build verified Home, canvas tap and visible drag scrolling
in stock DSH Web `0.2.1-alpha.1` and official Desktop `0.2.0-rc.2` on macOS
26.7 Apple Silicon / Xcode 26.4 / iOS 26.4 / iPhone 17. These local source checks
do not qualify the pending preview.5 archive. Native input effects and Web
tap coordinates after canvas resizing also passed on that target. Initial scope
is upright portrait, without keyboard/text/IME,
multi-touch, Back or iOS physical devices. Input is enabled only after a native
capability probe succeeds; otherwise preview stays read-only. Cleanup releases a
held iOS gesture by ending it, which can complete a tap or drag.
Other Xcode/runtime combinations and several lifecycle cases remain unqualified;
see [iOS Simulator evidence and remaining checks](docs/IOS-SIMULATOR.md).

For Android, the Rust backend and local DSH plugin provide device discovery,
explicit emulator startup, session ownership, H.264 streaming and
single-pointer/basic-key input.
The current candidate implements **Android 10–17 / API 29–37 arm64-v8a adapters**,
using a Unix host and a WebCodecs-capable client. Runtime qualification is still in
progress; eligibility is not a compatibility pass. Native capture/input/cleanup
has passed on the API 30–37 matrix targets, including an API 37 emulator with
16 KiB pages. Stock DSH Web GUI checks passed on API 30–34 with the local matrix
archive and on API 35–37 with the separately identified downloaded CI archive.
That CI archive also passed official Desktop GUI checks on API 37, including
input, Pause/Resume, window zoom, removal and reinstallation.
The tested API 29 emulator environment is blocked by its codec surface path.
Earlier API 32 emulator and API 36 physical-phone checks remain historical.
See the [Android compatibility matrix](docs/ANDROID-COMPATIBILITY.md) for per-target
results and gaps.
`transport_ready` remains a device connection state; the visible panel starts
preview automatically after a new connection, then reports actual decoding progress.

## Scope

| Capability | Status |
| --- | --- |
| Discover connected Android devices and installed AVDs | Implemented |
| Start an explicitly selected Android AVD | Implemented |
| Probe an exact serial and manage process-local session leases | Implemented |
| JSON-lines control protocol and bounded binary media framing | Implemented |
| Rust/JNI capture, NDK H.264 encoding and WebCodecs playback | API 29–37 arm64 adapters implemented; runtime qualification tracked per target in the compatibility matrix |
| Single-pointer input and basic Android keys | Implemented; Web/Desktop tap, drag and navigation verified on the target emulator |
| DSH Web/Desktop connection button and device panel | Implemented; stock Web and official Desktop GUI verified on the recorded archives and targets |
| Android/iOS platform picker and optional `open_mobile_preview` agent tool | Implemented; routes the current chat to its platform panel without choosing or connecting a device |
| iOS Simulator discovery, explicit startup and video | Experimental source implementation; previous-phase Web/Desktop video passes on Xcode 26.4 / iOS 26.4 / iPhone 17 |
| iOS Simulator single-pointer tap/drag and Home | Enabled when native input capability is verified; native, stock Web and official Desktop effects passed on the recorded target |
| iOS keyboard/text/IME, multi-touch, Back and physical devices | Not implemented |

Android physical devices can be discovered and probed through adb. Preview/input
qualification is separate from discovery and is recorded per tested target below.
Audio and recording
are outside the current scope. MPP draws on scrcpy's separation of video and
control, but does not run, redistribute or depend on scrcpy, and does not implement
its wire protocol.

## Install the preview candidate

The packaging script creates a self-contained `.tgz` for a **macOS Apple Silicon
DSH Host running macOS 14 or later**, with **Android API 29–37 arm64-v8a adapters**
and local **iOS Simulator** support through installed Xcode. iOS qualification is
scoped to macOS 26.7 / Xcode 26.4 / iOS 26.4 / iPhone 17. Physical iOS devices are
excluded. The `0.1.0-preview.5` archive identity and acceptance are tracked in the
[release checklist](docs/RELEASING.md); earlier package checks remain historical.
The
`0.1.0-preview.1` archive passed clean installation, playback and removal on the
official npm DSH `0.2.1-alpha.1` Web distribution in an isolated profile. This used
no source patch, runtime-path overrides or model API key. The official Desktop
`0.2.0-rc.2` app passed signature and notarization checks; `0.1.0-preview.3` was
installed and enabled through its bundled CLI in an isolated profile. The exact
downloaded CI preview.3 archive later passed that official app's GUI preview,
input and lifecycle checks on API 37. Earlier source-built Desktop checks remain
separate historical evidence.

With an archive built using [the packaging guide](docs/DSH-DEVELOPMENT.md#build-a-self-contained-preview-package),
open DSH **Plugins → Add plugin**, enter its absolute `.tgz` path, install it and
enable it. For a Web profile, the equivalent CLI command is:

```sh
dsh plugin --profile web add /absolute/path/mobile-dev-harness-dsh-mobile-preview-0.1.0-preview.5-darwin-arm64.tgz
```

`0.1.0-preview.5` is the selected release candidate. Use the exact archive whose
checksum and verification record appear in the release checklist; publication
has not yet been confirmed.

The archive includes the Rust host and matching Android bootstrap/native library;
their paths resolve relative to the installed package. No `MPP_EXECUTABLE`
environment variable, Rust, NDK or JDK is needed to use the archive. The DSH Host
needs Android SDK platform-tools and an authorized API 29–37 arm64 device for Android;
check its qualification status in the compatibility matrix before use.
Emulator use additionally requires the emulator package and an existing supported
AVD. Web clients need H.264 WebCodecs support. On stock DSH,
sidebar width remains manually adjustable; the optional width patch is not required
for preview or input.

Open the device panel and choose **Android** or **iOS**. A new chat starts without
a selected platform or adb discovery. **Android** loads the device inventory;
select the exact device and choose **Connect** to begin preview. In the current
source build on macOS, **iOS** lists local Xcode Simulators for explicit startup
and video, with tap/drag and Home when the selected Simulator's input probe passes;
otherwise video remains read-only. Other hosts report an unavailable backend.
Physical iOS devices are unsupported. Earlier qualified Android
archives do not include or qualify this new iOS implementation. See the
[package README](packages/dsh-plugin/README.md) for
prerequisites, limitations and removal. Explicit `executable` and `deviceAssets`
configuration is still supported for source development.

For the iOS Home gesture, start inside the bottom 2% of the actual screen and make
a short upward swipe. The decorative bezel is outside the touch surface. Gesture
qualification is in the [iOS guide](docs/IOS-SIMULATOR.md). A user-operated Web
App Switcher gesture passed on 2026-10-10; manual Desktop App Switcher remains
unqualified.

After installing a new native package version, restart the DSH Host before
connecting; hot reload can retain the previous package-relative runtime path.

The earlier `0.1.0-preview.1` packed Web check used the API 32 emulator and covered
automatic first frame, tap/drag, Home/Back, manual
sidebar resizing and Pause/Resume across panel reopening. Removing the connected
plugin unloaded its UI and left no MPP host, device bootstrap process or adb reverse
mapping. Local evidence is `target/stock-validation/evidence/stock-web-live.png`.
The packaging guide also describes GitHub Actions builds triggered by relevant
pushes or an explicit manual version. They upload workflow artifacts without
publishing a release. The downloaded preview.3 archive from run `37652914514`
(SHA-256 `d6f6450826638bdb44cce5a2b8669965036f7de87ec8d79e2468d3b5d08f9b9c`)
passed provenance, native API 37 and clean Web/Desktop CLI installation checks.
Stock Web `0.2.1-alpha.1` then decoded first frames on API 35–37 and verified
dragging, Settings/Apps taps and Back/Home. API 37 also passed Pause/Resume across
panel reopening and a 1280×820/narrow-viewport round trip while remaining Live.

Official Desktop `0.2.0-rc.2` decoded the same archive on API 37 and verified
Settings/Apps taps, dragging, Back/Home, Pause remaining paused across panel
reopening, Resume returning to Live and a native window-zoom round trip. Removing
the plugin through Plugin Manager removed its entry and renderer and cleaned its
dependency, bundle, helper and owned reverse mapping; GUI reinstallation and
enabling restored Live. Opening Plugin Manager hides the preview, so removal was
tested from a previously connected chat. Web GUI removal after the API 35 connected
session also removed the entry, replaced the device view with DSH's no-renderer
placeholder, cleared dependency/bundle entries and left no helper or owned reverse
mapping. The Web panel was hidden before entering Plugin Manager; GUI
reinstallation, enabling and reconnecting on API 35 restored a decoded Live view.
Evidence and screenshots are in `target/release-readiness/gui/`;
see the archive identities and remaining limits in the
[release checklist](docs/RELEASING.md).

The `0.1.0-preview.2` archive was installed in the official stock npm DSH Web
`0.2.1-alpha.1` distribution on macOS arm64. The nubia P0110 (Android 16/API 36,
arm64) displayed live video at 576×1280 from its 1264×2800 screen. Home/Back, tapping
Display & Brightness, dragging the settings list and Pause/Resume were verified.
Pause left no helper. Three separate native start/capture/stop cycles left no helper,
adb reverse mapping or MPP virtual display. The same new host/device assets also
passed the API 32 emulator capture/cleanup regression. Local evidence is
`target/android36/evidence/phone-live.png`. Physical-device rotation, hot unplug
and long GUI sessions were not tested in this increment. These phone results do
not qualify the later CI archive on physical devices or other vendors.

## Build and try from source

Requires Rust 1.88 or later. Android commands additionally require Android SDK
platform-tools (`adb`); AVD discovery and startup require the SDK emulator package
and an existing AVD. iOS Simulator commands instead require macOS Apple Silicon,
Xcode and an installed iOS Simulator runtime; Android SDK and device assets are
not needed for iOS preview.

```sh
cargo build --workspace --locked
./target/debug/mpp --version
./target/debug/mpp devices
./target/debug/mpp probe --device emulator-5554
```

Replace `emulator-5554` with a serial returned by discovery. Discovery and probing
never select a device implicitly or start one. To explicitly start an existing
AVD, use its exact name from `devices`:

```sh
./target/debug/mpp boot --avd Pixel_9
```

Startup is headless and waits for Android to finish booting. A successfully started
emulator remains running after the command or host exits; disconnecting a session
does not shut down the device.

SDK tools are resolved from `ANDROID_HOME`, `ANDROID_SDK_ROOT` and conventional SDK
locations before `PATH`. Explicit overrides are available:

```sh
./target/debug/mpp --adb /path/to/adb --emulator /path/to/emulator devices
```

When `--adb` is provided, also provide `--emulator` if AVD discovery or startup is
needed. Without the emulator tool, connected-device discovery still works and
returns a warning.

For the experimental iOS backend, use the exact Simulator UDID returned by
`devices`. Startup remains an explicit request:

```sh
./target/debug/mpp boot --simulator <UDID>
./target/debug/mpp probe --simulator <UDID>
```

The CLI accepts `--xcrun /absolute/path/to/xcrun`; the default is `/usr/bin/xcrun`.
The active Xcode installation supplies its runtimes and CoreSimulator components.
See the [iOS guide](docs/IOS-SIMULATOR.md) for source setup, capture boundaries and
scoped qualification. Disconnecting preview leaves the Simulator running.

## DSH development plugin

The adapter lives in `packages/dsh-plugin`. It runs without mdh and adds no
third-party Node dependencies; its UI uses DSH's runtime-provided React and UI
services. Source DSH `0.2.1-alpha.1` at commit
`5badb15009ae1756c3afe0ae0cef1faafc290ccc` is the base compatibility version.
Automatic phone-width sizing additionally uses a local DSH UI extension; it is
not part of that upstream commit or an officially merged API.

For Android, build the [device assets](#android-device-build-and-encoder-probe), then
prepare the pinned DSH source checkout and run either surface from this repository:

```sh
node scripts/dev-dsh.mjs web --source /path/to/deepseek-harness --port 3081
node scripts/dev-dsh.mjs desktop --source /path/to/deepseek-harness
```

The launcher uses separate development homes and Desktop user data under
`target/dsh/`. It does not copy credentials or settings from existing profiles or
require uninstalling the normal DSH app. See [DSH development](docs/DSH-DEVELOPMENT.md)
for Node/pnpm prerequisites, first-time builds, device assets and local plugin loading.

With the optional [sidebar-width patch](docs/DSH-DEVELOPMENT.md#automatic-sidebar-width-local-dsh-extension),
MPP requests a width from the available height, phone aspect ratio and surrounding
phone shell. DSH retains its layout limits and manual dragging takes priority.
Leaving the active docked panel, entering fullscreen, splitting or floating it
releases the temporary fit and restores the stored width preference. Unpatched DSH
keeps manual sidebar sizing; video and input are unchanged. The earlier width-only
Desktop check, before the phone shell, passed (95/100): at a 1280×820 CSS viewport,
the panel shrank from 576 to 300 CSS
pixels without reducing picture height. A manual 430-pixel width survived a change
in available height; the guide restored that preference and returning to MPP fitted
again. Fullscreen and window-height adaptation passed; split/float paths have unit
coverage, not separate live verification. Local evidence is
`target/dsh/evidence/desktop-adaptive-sidebar.png`.

The initial streaming qualification used the pinned DSH source without modifications.
Both its Web UI and native Harness Dev Desktop loaded the button/panel and listed
four local AVDs.
On `Pixel_6_API_32`, Web decoded the actual H.264 stream; a canvas tap opened
Settings → Apps, Back returned to Settings, dragging scrolled, and Home reached
the launcher. Local screenshot evidence is in `target/dsh/evidence/web-live-control.jpg`.

A native 600.1-second run remained healthy with the first 180 seconds static and
Home press/release pairs every 30 seconds afterward. It reported one configuration
packet, 8 keyframes, 228 delta frames and 1,184,670 bytes. Native checks also observed
touch release at the 2.6-second watchdog check and after control EOF, expected stream
termination on 90°/180° rotation, and 10/10 clean fresh start/stop cycles; device
settings were restored. This sparse-change run is not a throughput or latency benchmark.

Desktop decoded its first frame after the decoder-backpressure fix. Home moved
Calendar → launcher; dragging opened the app drawer; taps opened Settings → Apps;
Back returned to Settings. Sidebar resizing kept the preview live during about
two minutes of interaction. A checked stop left no MPP native helper or owned adb
reverse mapping. A clean Desktop process restart then reproduced a decoded live
frame without temporary diagnostics. The final live-frame screenshot is
`target/dsh/evidence/desktop-live-control.png`. Rejection of a competing capture
also left the active stream uninterrupted. An earlier unexplained Web EOF did not
recur during the native run or latest Web checks; its cause remains unresolved.
These results do not establish broader platform or sustained end-to-end GUI
reliability, and no throughput or latency claim is made.

After choosing **Android**, select an authorized device in the implemented API/ABI
range and choose **Connect**; the visible
phone screen starts preview automatically. A chat retains its preview intent through resizing,
temporary hiding/backgrounding and view replacement. Hiding suspends capture and
releases input; returning to a visible view resumes after the old capture is cleaned
up, provided the binding is still valid. **Pause** stays paused until **Resume**.
Genuine errors block repeated attempts in the current visible view. **Retry**,
**Resume**, or a new visible view/visibility return may make one fresh attempt
while intent and binding remain valid; ordinary rerenders and heartbeats do not
retry. Device rotation or geometry changes still end their capture epoch, and input
is never replayed. Disconnect, expiry and plugin unload clear the intent. An expired
device connection is not automatically reconnected.

The screen sits inside a rounded metal-style phone rim with an earpiece and
decorative side buttons. These are outside the actual screen and accept no input;
canvas aspect ratio and touch coordinates still refer to device pixels. Auto-fit
accounts for the rim, padding and outer gutter. This Connect/resume and phone-frame
update is MPP-only; it reuses the optional DSH width extension.

Desktop runtime checks reached Live after **Connect** alone. Tapping Apps in the
framed Settings screen opened Apps, and Home worked. Resizing and OS window zoom
kept Live with the same helper process within each check. Collapsing/reopening the
panel resumed automatically after cleanup without BUSY. Explicit Pause left zero
helpers and stayed paused across collapse/reopen; Resume returned to Live. Background
document events have unit coverage, but actual OS minimize/background return was
not separately tested. The earlier unchanged-height result predates the shell,
whose chrome now consumes some available height. Local evidence is
`target/dsh/evidence/desktop-auto-preview-device-frame.png`. Visual review passed
(96/100): the rim, earpiece and side buttons stay outside the screen, with no
stretching or decoration obscuring the status bar/app content. Controls remain
readable and the sidebar stays compact.

Connection leases are process-local; the native helper also rejects competing MPP
capture processes on the same device. This does not block external adb or physical
touches. Test the same device in Web and Desktop sequentially. Web operates on
devices attached to the DSH Host machine. Its stdio bridge is internal, not MCP.

When DSH provides its tools service, the plugin registers one optional agent tool:
`open_mobile_preview({platform: "android"})` (or `"ios"`). For example, ask the agent:
“Open the Android preview for this chat.” The tool selects a platform and requests
the panel for the calling chat; you still choose the device and connect. It cannot
boot or connect devices, send input or take screenshots, and grants no model vision.
An iOS request opens Simulator selection on macOS; the selected device's native
capabilities determine which manual controls are available. Manual use remains
available without the tools service.

Platform choices are kept in bounded DSH Host memory per chat. They survive a
browser reload while retained, but reset when the plugin service or DSH Host
restarts. A different-platform connection returns `BUSY`; disconnect it explicitly
before switching. While the document is visible, the mounted chat checks requests
every two seconds and opens its panel once per new agent request. An inactive chat
does not take focus. Closing the panel keeps it closed for that request; a new
agent request can reopen it.

## Local host contract

Start one persistent host process for clients that need sessions:

```sh
./target/debug/mpp serve --stdio
```

Send one JSON object per line. For example:

```json
{"id":"intro","method":"hello"}
{"id":"devices","method":"devices.list"}
{"id":"connect","method":"session.connect","params":{"owner":"chat-a","device":"android:emulator-5554"}}
```

Use the returned session ID and generation for subsequent status/disconnect calls.
All leases are released when the host exits or its input reaches EOF. Ownership is
enforced inside that process only; it does not lock other adb clients or device
touches. This is a local subprocess protocol, not MCP or a network API, and owner
strings are not authentication credentials.

Android leases also bind the adb transport ID, so a new attachment at a recycled
serial invalidates the old session. Restart MPP after restarting the adb server.

See [the control and media contracts](docs/CONTROL-PROTOCOL.md) for message shapes,
limits and unsupported operations, and [the design](docs/DESIGN.md) for boundaries
and the next increments.

## Development

The workspace contains five crates:

| Crate | Responsibility |
| --- | --- |
| `mpp-core` | Device/session types, capture epochs, input state and media framing; no platform I/O |
| `mpp-android` | SDK/device discovery, lifecycle, helper deployment and authenticated channel setup |
| `mpp-android-device` | Rust/JNI Android capture/input and NDK H.264 encoding |
| `mpp-ios` | Local Simulator discovery/startup, isolated CoreSimulator/IOSurface → VideoToolbox capture and capability-gated DTUHID input |
| `mpp-host` | `mpp` CLI, stdio lifecycle protocol and private media/control relays |

Tokio, Serde, serde_json and thiserror are the foundation dependencies; `jni` 0.22
and a tiny Java bootstrap were additionally approved for Android framework access.
Java initializes the runtime and loads Rust; capture, media, transport and input
remain in Rust. The JavaScript adapter binds sessions to DSH conversations and
provides the shared Web/Desktop UI.
The iOS crate adds no external dependency: it calls installed Apple frameworks
through a small native FFI boundary. Those frameworks are not redistributed.

```sh
cargo fmt --all --check
cargo clippy --workspace --all-targets --locked -- -D warnings
cargo test --workspace --locked
npm --prefix packages/dsh-plugin run check
node --test scripts/tests/*.test.mjs
```

Default tests require no device, Android SDK or network. They exercise protocol,
ownership, framing and subprocess behavior; passing them is not a live-video or
performance validation.

### Android device build and encoder probe

Build on macOS or Linux x86_64 with an installed Android NDK, the Rust
`aarch64-linux-android` target, JDK 17 or later, Android Build Tools with D8 and an
Android platform `android.jar` at API 29 or later. Native and DEX outputs target
minimum API 29. The shared `android_framework(api)` policy selects the API 29–33
or 34–37 implementation and requires arm64. The build verifies 16 KiB ELF LOAD
alignment; the separate API 37 matrix run verifies native behavior on one 16 KiB-page
emulator. The script uses
`--locked`, may fetch Cargo dependencies, and installs no SDK or Rust components:

```sh
./scripts/build-android-device.sh
```

The default profile is `debug`; set `MPP_BUILD_PROFILE=release` to build release
Android artifacts. The self-contained packaging script selects release for both
the host and Android assets.

Set `CARGO_NET_OFFLINE=true` to build without network access when dependencies are
already cached.

Set `ANDROID_NDK_HOME` and `JAVA_HOME` to select installed toolchains.
`MPP_BUILD_TOOLS` can select an absolute build-tools version directory;
`MPP_ANDROID_JAR` can select an absolute platform JAR. If the Rust target is under
a different toolchain, select it explicitly, for example
`RUSTUP_TOOLCHAIN=1.88 ./scripts/build-android-device.sh`. The script compiles the
bootstrap with `javac --release 8`, converts it with D8, and prints three artifacts:

```text
target/android-device/bootstrap.jar
target/android-device/libmpp_android_device.so
target/aarch64-linux-android/debug/examples/codec_probe
```

`CARGO_TARGET_DIR` relocates all outputs. Set `MPP_DEVICE_ASSETS` to the absolute
`android-device` output directory when launching DSH with relocated assets.

To run the probe, choose an authorized arm64 Android device running API 29 or later.
Read its numeric `transport_id` from `adb devices -l` and replace `1` below. Using
`-t` binds every command to that attachment, rather than a reusable serial. The
commands upload and then remove their probe file:

```sh
adb devices -l
MPP_TRANSPORT_ID=1
adb -t "$MPP_TRANSPORT_ID" push target/aarch64-linux-android/debug/examples/codec_probe /data/local/tmp/mpp-codec-probe
adb -t "$MPP_TRANSPORT_ID" shell chmod 700 /data/local/tmp/mpp-codec-probe
adb -t "$MPP_TRANSPORT_ID" shell /data/local/tmp/mpp-codec-probe
adb -t "$MPP_TRANSPORT_ID" shell rm /data/local/tmp/mpp-codec-probe
```

On success the probe reports JSON with `encoder`, `input_surface`,
`capture_verified: false` and optional `first_output` metadata. It exercises encoder
creation, its input surface and output polling; it does not attach the display to
that surface. Encoder initialization or output metadata proves neither screen
capture nor hardware acceleration. The actual preview uses the separate bootstrap
and native capture path; the probe's `capture_verified: false` is intentional.

Three consecutive probe runs on the `Pixel_6_API_32` emulator (Android API 32,
arm64-v8a) completed the encoder lifecycle, reporting `encoder: "video/avc"`,
`input_surface: true`, `capture_verified: false` and `first_output: null` each time.
Those historical probe runs verified configure/create-input-surface/start/dequeue/
stop/release only, not the current live-video path.

This source repository is public and in development; its crates have
`publish = false`. Source and staged plugin manifests retain `private: true`;
producing a local `.tgz` does
not publish it. MPP is licensed under [Apache-2.0](LICENSE); bundled dependencies
retain their own licenses, recorded in [THIRD-PARTY-NOTICES.md](THIRD-PARTY-NOTICES.md)
and `licenses/third-party/`, which are included in preview archives. GitHub Release
publication is authorized for `0.1.0-preview.5` and awaits final artifact acceptance;
npm publication is outside this release. The tested scope, remaining limits and
release status are in [RELEASING.md](docs/RELEASING.md). Official DSH distribution
is a future upstream contribution, not a current inclusion or endorsement.
This is a development preview, not a 1.0 stable release.
