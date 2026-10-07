# Rust framework

## Product boundary

Mobile Preview Plugin (MPP) is independent of mobile-dev-harness. It will provide
device connection, live preview and input for DSH Web and Desktop, with eventual
Android and iOS simulator and physical-device support. DSH UI code is a thin host
integration; device/session/media behavior belongs to Rust.

The current increment provides a DSH device connection panel, not a finished
screen-mirroring product. It implements real Android discovery, explicit emulator
startup, connection probing, device ownership, and a bounded local control protocol. A device-side
encoder prototype now exercises the Android NDK lifecycle, but host capture,
video streaming and input injection remain unsupported capabilities. No fake
frame is presented as a real device preview.

Audio, recording, remote device farms and a dependency on mdh are out of scope.
The implementation does not launch or redistribute scrcpy. Its separate media
and control paths inform the design; MPP uses its own versioned protocol and
Rust implementation, not scrcpy wire compatibility.

## Technology decisions

- Rust 2024, MSRV 1.88; crates remain unpublished.
- Tokio for asynchronous processes and local I/O, Serde/serde_json for control
  messages, thiserror for actionable typed errors. These dependencies were
  explicitly approved. No WebRTC, FFmpeg, scrcpy library or web framework yet.
- `mpp-core`: serializable device/capability models, exclusive session leases,
  input validation and bounded binary media framing. No platform I/O.
- `mpp-android`: argument-array subprocesses for adb/emulator discovery and
  lifecycle. A selected serial is probed; no implicit device selection or boot.
- `mpp-host`: the `mpp` executable and JSON-lines stdio control adapter. Its
  process owns all sessions and releases them on EOF. No unauthenticated network
  listener. DSH will start a dedicated process and expose authenticated UI routes.
- `mpp-android-device`: an Android-only NDK H.264 encoder wrapper and an independent
  codec probe. The initial build targets API 32 / arm64. It depends on core types,
  never on the host-side adb adapter. C ABI calls are confined to `codec.rs`;
  unsafe code remains forbidden in the existing crates and denied elsewhere in
  this new crate. The prototype adds no third-party dependencies.
- `packages/dsh-plugin`: an external Host/client plugin, using Node built-ins and
  DSH-provided React and UI modules. It has no npm runtime dependencies. Its
  factory-format client mounts a session-header button, an empty-chat composer
  button and a shared sidebar panel in source-built Web and Desktop.

### DSH connection integration

The development baseline is DSH `0.2.1-alpha.1`, source commit
`5badb15009ae1756c3afe0ae0cef1faafc290ccc`. The development launcher uses separate
Web/Desktop homes and Electron user data under `target/dsh`; it does not modify
DSH source or the user's ordinary profiles. Absolute-path profile entries let DSH
discover both plugin faces from the independent MPP package.

One adapter service owns a lazily started Rust host. DSH's authenticated Fetch
registry exposes `/api/mobile-preview/v1`; the browser never talks to an additional
unauthenticated listener. The service validates conversations through DSH's
metadata-only session observation, including saved conversations not loaded as
active agents. Random browser-client and connection bindings map to server-owned
Rust lease handles. Browser requests cannot supply Rust owners or generations.
Generations that JavaScript cannot represent exactly cause a protocol failure.

All Rust requests serialize, so a short status timeout cannot accidentally kill
an emulator boot ahead of it. Browser heartbeats run independently. The negotiated
defaults are a 15-second heartbeat and 45-second expiry; background throttling or
lost clients can expire a binding, requiring a fresh connection. Panel closure and
React unmount retain chat leases while the root client remains alive; disconnect,
conversation deletion, client expiry, plugin disposal and host failure clear them.
Process-local exclusivity does not lock a device against another DSH instance,
mdh, external adb commands or physical touches.

The panel displays actual inventory, startup progress and `transport_ready` status.
It explicitly marks video and input unavailable. It provides no MCP tools or model
vision, and does not send frames or device inventory into model context. The
adapter tests cover protocol failures, startup diagnostics, client ownership,
queue cancellation, expiry, reconnect and negotiated timing; the source-built Web
and native Desktop have both displayed actual emulator connections.

### Separate channels

Control uses versioned JSON request/response messages with request IDs and stable
error codes. Session ownership and generation checks prevent stale input from
being sent after reconnection. One host grants at most one owner per device;
this does not lock external adb clients or physical touches. The sequential host
finishes transport probing before reserving a lease, with no cancellation point
between reservation and readiness. A future concurrent host must use a
cancellation-safe reservation guard instead.

Android sessions bind the observed adb transport ID and AVD identity, not just
the reusable `emulator-5554` address. Status rechecks invalidate changed or missing
attachments. Transport IDs are scoped to an adb server lifetime; restart the MPP
host after restarting adb. Session handles must not survive a host restart.

Media uses a bounded binary frame protocol with configuration, key/delta video
packets, timestamps and dimensions. It is never base64 video in model context.
The first codec target is H.264; a decoder must regain a keyframe after loss or
reconfiguration. A protocol codec can be tested before a real capture backend
exists. No media endpoint is advertised by a backend that cannot produce it.

### Platform capture boundary

Android capture/encoding must use Android platform APIs. Rust on the host alone
cannot acquire a privileged display surface. A later spike must select the
device-side Rust/NDK implementation and the minimal platform bridge needed for
display capture and input; using scrcpy's Java server unchanged is not this plan.
macOS/iOS will need its own implementation and cannot reuse Android capture.
The current framework does not claim that simctl supplies arbitrary touch input.

The NDK prototype configures a surface-input AVC encoder, owns its codec/format/
window resources and bounds encoded output to 8 MiB before copying it. Dequeued
slots are released on success and validation errors. It uses `BufferInfo.size`,
because the NDK documents `offset` and the returned buffer capacity as invalid on
API 35 and earlier. Pure parameter and output-length checks run in host unit tests;
CI also cross-builds and lints the Android code so conditional FFI is checked.

The independent probe has passed three invocations on the Pixel_6_API_32 arm64
emulator: configure, create input surface, start, poll, stop and release succeeded.
All returned `capture_verified: false` and `first_output: null`. No producer was
attached, so this is not evidence of encoded frames, display capture, hardware
acceleration, latency or sustained throughput. Output-buffer copying and failure
cleanup still require device-level fault and real-frame tests.

The proposed next bridge is a minimal Java `app_process` entry point that loads
the Rust library. Rust would call API 32 SurfaceControl/InputManager through JNI
and feed the existing NDK encoder. This proposal, including the additional `jni`
dependency, awaits the user's language/dependency decision; it is not implemented.
Hidden platform APIs must be verified separately for each supported Android
version. If display dimensions or rotation change before reconfiguration is
implemented, capture and input must stop rather than use stale coordinates.

## First-increment acceptance

1. Build, format, lint and unit/integration tests pass without a device.
2. adb output parsing handles online, offline, unauthorized and emulator devices;
   no device is silently picked when more than one is present.
3. Starting an emulator is a separate explicit request, limited to an existing
   AVD. Discovery and connection probing never start or stop a device.
4. A session connection probes the exact serial and reports actual capabilities.
   Capture/input remain false and their requests return `UNSUPPORTED`.
5. Different owners cannot acquire the same device; stale owner/generation
   operations fail; disconnect and process shutdown release owned sessions.
6. Stdio serves multiple bounded requests, rejects malformed/oversized messages,
   and keeps diagnostics off stdout. Failed requests do not terminate the host.
7. Media protocol tests cover partial/coalesced reads, packet limits, malformed
   headers and round trips. These tests are not video performance measurements.
8. With an authorized emulator, validate discovery, get-state probing,
   connect/status/disconnect and unsupported media/input responses.

## Next increments

Next prove Android capture, H.264 encoding and input in Rust against one emulator,
then add authenticated media delivery and playback to the existing DSH panel.
Qualify USB Android devices separately. iOS discovery and capture/input
are a separate backend, with compatibility evidence tied to Xcode/runtime versions.
Upstream DSH inclusion is a later contribution, not a change to DSH in this increment.

## References

- [scrcpy architecture](https://github.com/Genymobile/scrcpy/blob/v4.1/doc/develop.md)
- [Android MediaCodec](https://developer.android.com/reference/android/media/MediaCodec)
- [DSH Desktop](https://github.com/deepseek-ai/deepseek-harness/blob/master/apps/desktop/README.md)
