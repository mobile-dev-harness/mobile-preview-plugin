# Mobile Preview Plugin

[简体中文](README.zh-CN.md)

Mobile Preview Plugin (MPP) is an independent Rust framework for connecting mobile
devices to DeepSeek Harness Web and Desktop. The intended product provides live
preview and input for emulators, simulators and physical devices, without requiring
mobile-dev-harness (mdh).

**Current status: Rust connection framework and local DSH development plugin.**
Android discovery, explicit emulator startup, transport probing and session
ownership are available through the CLI and a shared DSH Web/Desktop button and
panel. Live video and input injection are not implemented. Connecting returns
`transport_ready`: the selected adb transport was checked and reserved, but no
video stream is available.

## Scope

| Capability | Status |
| --- | --- |
| Discover connected Android devices and installed AVDs | Implemented |
| Start an explicitly selected Android AVD | Implemented |
| Probe an exact serial and manage process-local session leases | Implemented |
| JSON-lines control protocol and bounded binary media framing | Implemented |
| Android NDK encoder lifecycle and output wrapper | Prototype; independent encoder probe |
| Display capture, live video delivery/playback and device input | Not implemented |
| DSH Web/Desktop connection button and device panel | Implemented; tested against the pinned source development baseline |
| iOS Simulator and iOS physical devices | Planned; no backend yet |

Android physical devices can be discovered and probed through adb today; this does
not establish support for physical-device preview or control. Audio and recording
are outside the current scope. MPP draws on scrcpy's separation of video and
control, but does not run, redistribute or depend on scrcpy, and does not implement
its wire protocol.

## Build and try

Requires Rust 1.88 or later. Device commands additionally require Android SDK
platform-tools (`adb`); AVD discovery and startup require the SDK emulator package
and an existing AVD.

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

## DSH development plugin

The adapter lives in `packages/dsh-plugin`. It runs without mdh and adds no
third-party Node dependencies; its UI uses DSH's runtime-provided React and UI
services. Source DSH `0.2.1-alpha.1` at commit
`5badb15009ae1756c3afe0ae0cef1faafc290ccc` is the current compatibility baseline.

After preparing that source checkout, run either surface from this repository:

```sh
node scripts/dev-dsh.mjs web --source /path/to/deepseek-harness --port 3081
node scripts/dev-dsh.mjs desktop --source /path/to/deepseek-harness
```

The launcher uses separate development homes and Desktop user data under
`target/dsh/`. It does not copy credentials or settings from existing profiles or
require uninstalling the normal DSH app. See [DSH development](docs/DSH-DEVELOPMENT.md)
for Node/pnpm prerequisites, first-time builds and local plugin loading.

The pinned source built successfully without DSH source changes. Both its Web UI
and native Harness Dev Desktop loaded the button/panel and listed four local AVDs.
The Web panel started `Pixel_6_API_32`, connected and disconnected it; the native
Desktop panel also reached `transport_ready`. This verifies connection behavior,
not streaming or device input. The adapter's 56 tests pass alongside the existing
52 Rust tests; broader lifecycle and platform qualification remain separate checks.

Ownership is process-local, so test the same device in Web and Desktop sequentially.
Web operates on devices attached to the DSH Host machine. The panel registers no
agent tools and does not grant model vision; its stdio bridge is internal, not MCP.

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

The workspace contains four crates:

| Crate | Responsibility |
| --- | --- |
| `mpp-core` | Device types, session leases, input validation and media framing; no platform I/O |
| `mpp-android` | Android SDK discovery and bounded adb/emulator subprocesses |
| `mpp-android-device` | Device-side Rust NDK MediaCodec lifecycle, input surface and encoded-output wrapper |
| `mpp-host` | `mpp` CLI and persistent stdio host |

Tokio, Serde, serde_json and thiserror are the approved foundation dependencies.
Rust owns device and media behavior. The small JavaScript adapter binds its sessions
to DSH conversations and provides the shared Web/Desktop UI.

```sh
cargo fmt --all --check
cargo clippy --workspace --all-targets --locked -- -D warnings
cargo test --workspace --locked
npm --prefix packages/dsh-plugin run check
```

Default tests require no device, Android SDK or network. They exercise protocol,
ownership, framing and subprocess behavior; passing them is not a live-video or
performance validation.

### Android encoder probe

The device-side prototype can be cross-built on macOS or Linux x86_64 for Android
API 32 or later, `aarch64-linux-android`; only API 32 has been tested. It requires
an installed Android NDK and the Rust target for the selected compiler. The script
uses `--locked` and may fetch locked Cargo dependencies; it installs neither SDK
components nor Rust toolchains/targets:

```sh
./scripts/build-android-device.sh
```

Set `CARGO_NET_OFFLINE=true` to build without network access when dependencies are
already cached.

Set `ANDROID_NDK_HOME` to select an installed NDK. If the target is installed under
a different toolchain, select it explicitly, for example
`RUSTUP_TOOLCHAIN=1.88 ./scripts/build-android-device.sh`. The script prints paths
to `libmpp_android_device.so` and the `codec_probe` executable; by default they are
under `target/aarch64-linux-android/debug/`.

To run the probe, choose an authorized arm64 Android device running API 32 or later.
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
capture nor hardware acceleration. The platform capture bridge remains undecided;
`preview.start` and `input.send` in the host still return `UNSUPPORTED`.

Three consecutive probe runs on the `Pixel_6_API_32` emulator (Android API 32,
arm64-v8a) completed the encoder lifecycle, reporting `encoder: "video/avc"`,
`input_surface: true`, `capture_verified: false` and `first_output: null` each time.
This verifies configure/create-input-surface/start/dequeue/stop/release only; no
encoded frames or display capture were demonstrated.

This repository is in development and its crates have `publish = false`. No public
package release or license has been selected. Official DSH distribution is a future
upstream contribution, not a current inclusion or endorsement.
