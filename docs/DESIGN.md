# Rust framework and mobile preview

## Product boundary

Mobile Preview Plugin (MPP) is independent of mobile-dev-harness. It will provide
device connection, live preview and input for DSH Web and Desktop, with eventual
Android and iOS simulator and physical-device support. DSH UI code is a thin host
integration; device/session/media behavior belongs to Rust.

The `0.1.0-preview.5` GitHub Release candidate includes existing Android support
and local iOS Simulator on macOS Apple Silicon. Physical iOS is outside this
release. The repository is public; npm stays private. Final artifact acceptance
and publication are pending in [RELEASING.md](RELEASING.md).

The current development implementation includes Android discovery, explicit
emulator startup, transport leases, display capture, H.264 transport and
single-pointer/basic-key control in the DSH panel. Capture is deliberately gated
to Android API 29–37 on arm64-v8a and a Unix host, using two framework paths.
This is an implemented eligibility range, not a completed runtime qualification
matrix. The [Android compatibility record](ANDROID-COMPATIBILITY.md) separates
current passes, environmental blockers and untested targets. Earlier API 32
emulator and API 36 physical-device checks remain historical. Broader platform
and long GUI-session qualification remain outstanding.
A transport connection does not establish a decoded first frame or a successful
application action.

The current source also implements experimental iOS Simulator video and
capability-gated single-pointer tap/drag and Home on macOS Apple Silicon with Xcode.
Initial scope is upright portrait, without keyboard/text/IME, multi-touch, Back
or physical iOS devices. Native and interactive stock Web/official Desktop
Home/tap/drag effects passed on the recorded target. The previous read-only source build decoded
changing Settings video in stock DSH Web and official Desktop on macOS 26.7 Apple
Silicon / Xcode 26.4 / iOS 26.4 / iPhone 17. Other versions and several lifecycle
cases remain unqualified; see the [iOS qualification record](IOS-SIMULATOR.md).
This source increment is separate from the previously qualified Android archives.

The source bottom-edge Home gesture passed in Web and Desktop. A manual Web App
Switcher gesture passed on 2026-10-10; manual Desktop App Switcher is unqualified.
These observations apply to the recorded Xcode 26.4 / iOS 26.4 / iPhone 17 target,
not every runtime or the pending release archive.

Audio, recording, remote device farms and a dependency on mdh are out of scope.
The implementation does not launch or redistribute scrcpy. Its separate media
and control paths inform the design; MPP uses its own versioned protocol and
Rust implementation, not scrcpy wire compatibility.

## Technology decisions

- Rust 2024, MSRV 1.88; crates remain unpublished.
- Tokio for asynchronous processes and local I/O, Serde/serde_json for control
  messages, thiserror for actionable typed errors, and approved `jni` 0.22 for
  Android framework calls. No WebRTC, FFmpeg, scrcpy library or web framework.
- `mpp-core`: serializable device/capability models, exclusive session leases,
  capture identities, input state and bounded binary media framing. No platform I/O.
- `mpp-android`: argument-array subprocesses for adb/emulator discovery and
  lifecycle, exact-attachment asset deployment, owned reverse tunnels and device
  channel authentication. No implicit device selection or boot.
- `mpp-host`: the `mpp` executable and JSON-lines stdio control adapter. Its
  process owns sessions and preview tasks, and releases them on EOF. Separate
  private Unix sockets carry video and input; these bypass the stdio lifecycle queue.
- `mpp-android-device`: Rust/JNI display capture and input plus NDK H.264 encoding,
  restricted to API 29–37 / arm64. It depends on core types, never the adb adapter.
  Its audited unsafe boundaries are limited to codec, platform and native modules.
  `codec_probe` remains independent.
- `mpp-ios`: local Simulator discovery/startup, capture and capability-gated input in an isolated
  child. Its macOS native boundary uses direct Objective-C runtime FFI,
  CoreSimulator/IOSurface, pixel transfer, VideoToolbox and DTUHID; no external dependency
  or Apple framework binary is added to the package.
- `android-bootstrap/dev/mpp/Bootstrap.java`: approved tiny `app_process` entry
  that prepares the Looper, loads the Rust shared library, invokes its native
  entry and exits. It contains no capture, transport or input implementation.
- `packages/dsh-plugin`: an external Host/client plugin, using Node built-ins and
  DSH-provided React and UI modules. It has no npm runtime dependencies. Its
  factory-format client mounts a session-header button, an empty-chat composer
  button and a shared sidebar panel in source-built Web and Desktop.

### DSH connection integration

The base development version is DSH `0.2.1-alpha.1`, source commit
`5badb15009ae1756c3afe0ae0cef1faafc290ccc`. The development launcher uses separate
Web/Desktop homes and Electron user data under `target/dsh`; it does not modify
the user's ordinary profiles or apply source patches automatically. Absolute-path
profile entries let DSH discover both plugin faces from the independent MPP package.
Streaming/control initially qualified on the unmodified source. The subsequent
optional sidebar-sizing feature uses the local UI extension described below.

One adapter service owns a lazily started Rust host. DSH's authenticated Fetch
registry exposes `/api/mobile-preview/v1`; the browser never talks to an additional
unauthenticated listener. The service validates conversations through DSH's
metadata-only session observation, including saved conversations not loaded as
active agents. Random browser-client and connection bindings map to server-owned
Rust lease handles. Browser requests cannot supply Rust owners or generations.
Generations that JavaScript cannot represent exactly cause a protocol failure.

Stdio lifecycle requests serialize, so a short status timeout cannot accidentally
kill an emulator boot ahead of it. Live input uses a dedicated socket and does not
wait behind that queue. Browser client heartbeats run independently. The negotiated
defaults are a 15-second heartbeat and 45-second expiry; background throttling or
lost clients can expire a binding, requiring a fresh connection. Panel closure and
React unmount retain chat leases while the root client remains alive; disconnect,
conversation deletion, client expiry, plugin disposal and host failure clear them.
Transport leases remain process-local. An Android abstract-socket lock additionally
prevents competing MPP capture helpers on one device; it does not block mdh,
external adb commands or physical touches.

The panel begins with Android/iOS selection. New chats have no default platform
and opening the landing view does not trigger adb discovery. Android selection
loads Android inventory; iOS selection on macOS loads local Xcode Simulators for
explicit startup and video. Native probing gates single-pointer tap/drag and Home;
missing input capability leaves read-only video. Non-macOS Hosts show an unavailable
backend for iOS. Physical iOS devices remain unimplemented.
The Android panel displays inventory, startup progress and `transport_ready`
status. A new binding enables per-chat preview intent, and a visible mounted
canvas starts it automatically. Actual start checks assets/device and WebCodecs probes its codec.
Hiding suspends capture/input while retaining intent and a valid transport lease;
returning to an eligible view resumes it. Explicit Pause/Resume remains distinct
from a visibility suspension, and genuine errors block repeated current-view
attempts. The plugin provides no MCP tools or model vision and sends neither frames
nor inventory into model context. Its optional agent tool only routes the chat's
platform panel, as described below. The adapter tests cover protocol failures,
startup diagnostics, client ownership,
queue cancellation, expiry, reconnect and negotiated timing; the source-built Web
and native Desktop have both displayed actual emulator connections.

### Platform selection and agent panel requests

Platform selection belongs to the JavaScript DSH integration; it does not change
the Rust protocol or native backend. Authenticated `platform.get` and
`platform.select` methods on the existing browser route validate the chat and
access one bounded host-owned selection cache. A record contains
`{sessionId, platform, source, revision, epoch, available}`. Platform starts as
`null`; `android` and `ios` are the only selections. Source is `null`, `user` or
`agent`. Android reports backend availability; iOS reports availability on macOS.
Availability is not an installed-Xcode/runtime, device compatibility or decoded-frame
claim. The six selection fields and optional agent tool contract are unchanged.

The cache holds at most 256 chats and evicts least-recently-used records. It is
independent of browser clients and device leases: a browser reload recovers a
retained choice, while plugin-service disposal or DSH Host restart clears it.
Every selection advances a host revision, including another request for the same
platform; a fresh service epoch distinguishes host lifetimes. A pending connection
or a different-platform device already bound to the chat yields `BUSY`. Selection
never disconnects an existing device to make room.

One optional DSH agent tool, `open_mobile_preview`, accepts only
`{platform: "android" | "ios"}`. It obtains the conversation from
`exec.agent.session.id` and uses the same validated selection operation with
source `agent`. It only selects the platform and requests that chat's panel; it
cannot list or choose devices, boot an emulator, connect, send input or capture
screenshots. Users still pick the actual device and connect. Registration uses
`ctx.inject(['tools'], ...)` so the base plugin works when no tools service is
present. No additional dependency or MCP server is introduced.

Only the mounted chat polls platform state, every two seconds while the document
is visible. It opens its panel once for each new agent epoch/revision, preventing
an inactive chat from taking focus. Closing a panel is respected for the consumed
request; a later request can reopen it. Polling routes the panel; existing connected
previews still follow the visibility rules below. The tool's successful return
records a UI request, not delivery to an absent frontend or a live device session.

### Preview intent and phone presentation

Each chat separates wanted preview intent, the currently eligible canvas view,
and its native capture runtime. New bindings default to wanted. Visible mount,
page return and view replacement reconcile those states; normal CSS resizing does
not revoke intent. Hiding/pagehide quiesces decoding and input, then cleans up the
old capture while retaining intent. A new eligible view waits for that cleanup
before starting, including when an obsolete start response arrives late. A disposed
old view cannot restart itself or close a replacement view's capture.

Explicit Pause clears wanted intent until Resume. Genuine failures latch within
the current visible view/visibility period; ordinary rerenders and heartbeats do
not clear the latch. Explicit Resume/Retry or a new visible view/visibility return
can make one new attempt while intent and binding remain valid. There is no timer
retry or hot retry loop, and no input is replayed. Disconnect, binding/client expiry and plugin unload clear
intent; this is not automatic device reconnection or durable state across expiry.
Native rotation/display changes still end their capture epoch and use the same
bounded retry/visibility-return rule.

The CSS phone body has a rounded metal-style rim, earpiece and decorative side
buttons. Decorations remain outside the device screen and have no pointer events.
The canvas retains the encoded screen aspect ratio and coordinate mapping; chrome
is not part of capture geometry. The sizing budget is a 4-pixel outer gutter per
side, a 2-pixel rim, and 8/10-pixel horizontal/vertical padding. Fit calculations
remove vertical chrome before applying the screen ratio, then add horizontal
chrome and section padding, rather than multiplying the full shell height by the
bare screen ratio. The 9/20 fallback remains for unknown screen geometry.

These lifecycle/presentation changes are in MPP and reuse the existing local DSH
width extension; they require no further DSH source changes. Desktop runtime checks
now cover Connect directly reaching Live, an Apps tap within the framed Settings
screen, and Home. Reported window widths 2560→2700 stayed Live with one helper PID;
a separate OS zoom check 2700→3024 and back also retained its helper PID. These are
reported window dimensions, not the earlier CSS-viewport measurement.

Panel collapse/reopen resumed Starting→Live with a new helper only after cleanup,
without BUSY. Explicit Pause produced a paused state and zero helpers, persisted
through collapse/reopen, and Resume restored Live. Lifecycle/frame unit coverage
also passes, but actual OS minimize/background-return was not separately exercised;
document background events are unit-tested. The screenshot is
`target/dsh/evidence/desktop-auto-preview-device-frame.png`. Visual review passed
(96/100): the rim, earpiece and side buttons remain outside the screen without
stretching or obscuring status-bar/app content; controls remain readable and the
sidebar compact. Earlier width-only screenshots remain historical, and
their unchanged-picture-height result does not apply to chrome that consumes height.

### Temporary sidebar width requests

The pinned upstream DSH does not expose content-driven sidebar sizing. A local
additive extension on `feat/sidebar-content-width` supplies
`ctx.layout.requestRightbarWidth(width)` with `update(width)`/`dispose()` and
`tabInfo.panel.isSoleDockedPane`. It is not an upstream public API or an official
release. The patch and pinned-source rebuild workflow are documented in
[DSH development](DSH-DEVELOPMENT.md#automatic-sidebar-width-local-dsh-extension).

MPP participates only while its tab is visible in the sole docked pane, outside
fullscreen. Once mounted, it requests width from the screen height remaining after
subtracting phone chrome, then adds horizontal chrome and section padding; before
geometry arrives the aspect fallback is 9/20. The
host still applies its 300-pixel right-panel minimum, 70%-of-frame ceiling and
400-pixel center reservation. Small windows can therefore prevent an exact fit.

A manual user drag invalidates the active width request. MPP retains that handle
and does not recreate it to fight subsequent user sizing. On panel departure,
fullscreen, split or float transitions, the temporary request is disposed and
the stored width preference takes effect. Unpatched DSH falls back to manual
width with media/control unchanged.

The earlier width-only local-extension Desktop review passed (95/100), before the
phone shell and automatic-resume update. At 1280×820 CSS pixels,
the panel automatically narrowed from 576 to the 300-pixel minimum while picture
height stayed unchanged. Manual dragging to 430 pixels survived expanding connection
details and its resulting preview-height change. Switching to the Start guide
restored the saved 430-pixel preference; returning to MPP fitted again. Fullscreen
kept full viewport width, window zoom/height changes adapted, and the normal window
was restored. Split/float paths passed unit tests but were not separately exercised
live. Evidence: `target/dsh/evidence/desktop-adaptive-sidebar.png`. Capture suspends
when hidden/occluded; wanted intent permits a fresh attempt on visibility return
only while the binding remains valid and the user has not explicitly paused.

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

Media uses MPP1 binary configuration/key/delta packets and H.264 Annex B access
units, not base64 data in model context. A per-capture epoch is separate from the
lease generation carried in the packet header. Browser stream tokens map to exact
captures; old stops and late callbacks cannot control a new capture.

The Rust host first opens a loopback listener and creates an owned adb reverse
mapping. The native process opens separate video/control channels with role, token,
lease, epoch and geometry handshakes. Both roles must agree; each receives its ACK
before the host waits for the other. The reverse mapping is removed after connection.
Secrets travel in bounded stdin configuration, never process arguments or logs.
The Java bootstrap receives only a native library path as its argument.

The Node adapter owns a mode-0700 short temporary directory containing the Rust
video/control sockets. Authenticated streaming Fetch delivers binary video;
authenticated bounded POST batches deliver input independently. Socket closures
precede queued Rust cleanup. Only owned helper files and mappings are removed;
successful emulator startup is not undone when a preview or host closes.

### Platform capture boundary

Rust uses JNI for Android framework access and NDK MediaCodec for encoding. Host
and native eligibility checks use the shared `android_framework(api)` policy and
retain the arm64 requirement. APIs 29–33 select SurfaceControl/InputManager; APIs
34–37 select DisplayManager/InputManagerGlobal; versions outside that range are
rejected. Minimum NDK/DEX API 29 and adapter presence are not runtime-test passes.
Discovery/probing has a wider scope than live capture and is not evidence of
preview compatibility.

APIs 29–33 use the `SurfaceControl` capture transaction and `InputManager` input
path. APIs 34–37 call the hidden static
`DisplayManager.createVirtualDisplay(String, int, int, int, Surface)` mirror
overload to mirror display 0 into the encoder surface. Its capture owner retains
the returned `VirtualDisplay` and calls `release()` before releasing the encoder
surface; it does not fall back to legacy capture. Input uses
`InputManagerGlobal.getInstance()` and `injectInputEvent(InputEvent, int)`. These
signatures were checked against the tagged AOSP Android 16 sources:
[DisplayManager](https://raw.githubusercontent.com/aosp-mirror/platform_frameworks_base/android-16.0.0_r1/core/java/android/hardware/display/DisplayManager.java),
[VirtualDisplay](https://raw.githubusercontent.com/aosp-mirror/platform_frameworks_base/android-16.0.0_r1/core/java/android/hardware/display/VirtualDisplay.java),
and [InputManagerGlobal](https://raw.githubusercontent.com/aosp-mirror/platform_frameworks_base/android-16.0.0_r1/core/java/android/hardware/input/InputManagerGlobal.java).

The Java bootstrap is unchanged; API-specific capture, input and cleanup stay in
Rust. Hidden framework signatures and vendor behavior still require target-device
qualification. The separate iOS Simulator input path uses DTUHID; simctl is not
assumed to provide arbitrary touch injection.

The iOS capture child samples raw IOSurface pixels at every bounded frame interval,
copies/scales into an owned NV12 pixel buffer with VTPixelTransfer, and encodes
through VideoToolbox into the existing MPP1/WebCodecs path. GPU rendering does not
reliably advance IOSurface seeds; seed checks only detect some concurrent changes
during copying and never gate sampling. This is raw surface acquisition, not a
PNG/screenshot loop. Direct Objective-C runtime
FFI checks the private CoreSimulator classes, protocols and selectors it needs.
Those interfaces require qualification per Xcode/runtime version; the current
machine's installed CoreSimulator is 1174.9.2, not a claimed universal baseline.

The current `mpp` executable starts its own `--ios-capture` child with a full-duplex
Unix control socket inherited as FD 0 (`--control-fd 0`); stdout carries MPP1 and
stderr diagnostics. `--enable-input` is added only after a positive native probe.
It binds the Simulator UDID plus `launchd_sim` PID and libproc process start
time, validates that boot at startup and periodically, and terminates on identity,
orientation or surface geometry changes. EOF/SIGTERM cleanup releases owned
capture/encoding resources without shutting down the user's Simulator. The
six-field preview descriptor remains unchanged; Node propagates the probed
`Device.capabilities.input`, preserving a read-only fallback. iOS startup needs
no Android JAR or native `.so`.

CoreSimulator 1174.9.2 suppresses the legacy Indigo input path. The native backend
uses the DTUHID digitizer XPC service through the selected device's Mach port.
A positive no-event handshake gates input capability. Real events are sent with
`isBarrier: false`, followed by a no-event barrier whose reply confirms ordered
submission. HTTP success waits for that native acknowledgement, not merely queueing;
application effects still require observation. Input validates boot identity,
geometry, sequence and epoch, and rechecks timed-out work before injecting.
Its wakeup is independent of video FPS. The two-second watchdog, reset, EOF and
owner drop release owned holds. DTUHID `cancel` maps to end, which may complete a
tap or drag; it does not provide UIKit `touchesCancelled` semantics.

The API 29 build links with 16 KiB maximum/common page-size alignment and inspects
every ELF LOAD segment before replacing device assets. The checked library had
four 16 KiB-aligned LOAD segments. Separately, the API 37 matrix target passed
native capture/input/cleanup with an observed 16,384-byte page size; other 16 KiB
targets remain unqualified. Packaged assets use `native/android-arm64`, and manifest
`supportedApis` records adapter policy rather than completed matrix results.

The native backend configures a surface-input AVC encoder, owns its codec/format/
window resources and bounds encoded output to 8 MiB before copying it. Dequeued
slots are released on success and validation errors. It uses `BufferInfo.size`,
because the NDK documents `offset` and the returned buffer capacity as invalid on
API 35 and earlier. Pure parameter and output-length checks run in host unit tests;
CI also cross-builds and lints the Android code so conditional FFI is checked.

The independent probe has passed three invocations on the Pixel_6_API_32 arm64
emulator: configure, create input surface, start, poll, stop and release succeeded.
All returned `capture_verified: false` and `first_output: null`. The probe attaches
no producer and is not the live-preview path. Its result proves neither display
capture nor hardware acceleration, latency or sustained throughput.

The Java bootstrap and `jni` dependency were approved on 2026-10-07 and are now
implemented. Rotation or display geometry changes terminate capture and input;
the user starts a new preview with a new epoch. Input supports one pointer and
Android key codes, not arbitrary text or IME input. Browser-origin control
heartbeats run every 750 ms while controls are held; the device cancels pressed
state after two seconds without valid controller traffic. Host transport ownership
never synthesizes input heartbeats or sequence numbers.

## Qualification gates

1. Build, format, lint and unit/integration tests pass without a device.
2. adb output parsing handles online, offline, unauthorized and emulator devices;
   no device is silently picked when more than one is present.
3. Starting an emulator is a separate explicit request, limited to an existing
   AVD. Discovery and connection probing never start or stop a device.
4. A session connection verifies its exact adb attachment. Connection capabilities
   do not substitute for asset, platform, codec and first-frame checks.
5. Different owners cannot acquire the same device; stale owner/generation
   operations fail; disconnect and process shutdown release owned sessions.
6. Stdio serves multiple bounded requests, rejects malformed/oversized messages,
   and keeps diagnostics off stdout. Failed requests do not terminate the host.
7. Media protocol tests cover partial/coalesced reads, packet limits, malformed
   headers and round trips. These tests are not video performance measurements.
8. For each allowed API/arm64 target, validate decoded display changes and
   visible input effects. Complete rotation, backpressure, interrupted control,
   cleanup, ten reconnects and sustained-session checks before stability claims.

## Next increments

Extend physical-device qualification to rotation, hot unplug and long GUI sessions,
then qualify additional USB Android devices and Android API/ABI combinations separately.
Complete the remaining iOS lifecycle qualification on the recorded target, then
widen the Xcode/runtime/Simulator compatibility matrix.
Extend input lifecycle qualification beyond the recorded Web/Desktop passes. Keyboard/text/IME,
multi-touch, Back and physical-device support remain separate future work.
Upstream DSH inclusion is a later contribution, not a change to DSH in this increment.

## Streaming and input: research basis and implementation rules

The reference findings below informed MPP's own implementation; they do not
establish its device compatibility or performance. The reference is
scrcpy v4.1, commit `2926c06c5dc3064ae6d8db706f1a98a37cfcf3f0`, and the DSH base is
`5badb15009ae1756c3afe0ae0cef1faafc290ccc` with the optional local UI extension above.
Tagged source takes precedence
over examples in scrcpy's developer document that still mention version 4.0.

### Findings that affect MPP

| Verified reference behavior | MPP implication |
| --- | --- |
| Android display mirroring feeds a MediaCodec input Surface; screen transforms can add an OpenGL stage. | Start with the main display and platform encoding, without a screenshot loop, crop filters or a desktop transcoder. Do not assume hardware encoding on every device. |
| Compressed packets are consumed in order; `frame_buffer.c` replaces pending decoded AVFrames. | Drop superseded decoded pictures; bound ordered encoded reads and wait for decoder capacity. On failure, stop or resynchronize at configuration plus a keyframe; never keep dependent delta frames after discarding their references. |
| The encoder holds an output buffer while the synchronous socket write completes. | Preserve MPP's prompt copy/release of the codec slot, then apply explicit byte and time bounds to transport queues. |
| Reverse tunneling is the default; forward mode adds a first-socket readiness byte. Socket order is video, optional audio, then control. | Use selected adb transport IDs, unique owned artifact/socket names, explicit channel-role/version handshakes, startup deadlines and cancellation. Readiness metadata is not authentication. |
| Coordinates carry frame dimensions; desktop mapping removes letterboxing and applies transforms. | Add a capture/geometry epoch as well as lease generation: equal dimensions cannot distinguish a 180-degree rotation. Reject black-bar presses and stale transforms. |
| The SDK controller queue may drop touch/key edges when full; ordinary input has no application-completion acknowledgement. | Coalesce only intermediate moves, preserve gesture edges, track pressed inputs, and distinguish queued/submitted/rejected receipts from application success. |
| Normal shutdown interrupts sockets before joining processors; cleanup has a separate lifetime. | Give each Rust device session one supervisor; EOF, host death, expiry and view cancellation must stop work and release pressed inputs without touching unrelated devices. |

Primary source anchors: [capture][scrcpy-capture], [encoding][scrcpy-encoder],
[decoded frame replacement][scrcpy-frame-buffer], [tunneling][scrcpy-tunnel],
[connection setup][scrcpy-connection], [input queue][scrcpy-controller],
[droppable events][scrcpy-droppable], [coordinate mapping][scrcpy-position],
and [device shutdown][scrcpy-server].

### Channels and ownership

The Android video path is device display → Rust/NDK encoder → ordered bounded media
transport → Rust host → thin DSH byte forwarding → WebCodecs decoder → canvas.
The iOS Simulator path replaces capture/encoding with IOSurface → owned NV12 →
VideoToolbox in its isolated child, preserving the media and browser path.
The Rust host and Node adapter do not decode or re-encode video. Video is displayed
in the device panel and is not continuously supplied to a language model.

MPP uses an authenticated streaming Fetch response for browser video. The pinned DSH
gateway WebSocket stream is JSON/text and rejects binary input; its unary multipart
attachments are not a raw video-stream carrier. DSH's HTTP bridge supports response
backpressure and propagates cancellation, and Desktop forwards the response body.
The producer must still observe abort signals and close its owned stream: the HTTP
bridge may drain a cancelled response. Verify this in both actual runtimes.

For the Unix host, private temporary Unix sockets connect the Rust media/control
relays to Node's built-in stream adapters. Channel setup binds
the selected device attachment, lease, capture epoch and peer. Device adb channels
need separate readiness and peer authentication; neither loopback nor an adb
tunnel is a substitute for DSH browser authorization. Windows transport is a
separate implementation requirement, not implied by this Unix design.

Device control uses a persistent Rust worker/channel independent of video and
slow lifecycle jobs. Authenticated POST requests carry bounded, ordered batches;
the browser coalesces adjacent movement without crossing gesture edges. These
batches go to the dedicated control worker, not the stdio/lifecycle queue.
Input with an uncertain result is not automatically retried.
An authenticated browser WebSocket can be evaluated if measured HTTP overhead
warrants the additional transport work; it is not a prerequisite for the first
local control loop.

Input uses a controller binding and strictly increasing action sequence numbers.
The native `mpp-control-owner-v1` abstract socket excludes a second MPP capture
process on the device. External adb tools and physical touches remain outside
that exclusion; test the Web and Desktop clients sequentially against one device.

### Decoder, geometry and release rules

For WebCodecs Annex B input, omit `VideoDecoderConfig.description`, derive the codec
string from the actual SPS, and include required SPS/PPS with a recovery key access
unit. Configuration-only packets are not pictures. Probe the actual codec and
dimensions in Web and Electron, then handle runtime decoder errors as well.
Bound compressed backlog and `decodeQueueSize`; retain at most one pending decoded
`VideoFrame`, closing replaced and displayed frames. Queue size alone is not a
measurement of total decoder memory. See the [AVC registration][webcodecs-avc]
and [WebCodecs specification][webcodecs].

Repeated identical SPS/PPS retain the configured decoder and pending canvas frame.
An encoder repeating its configuration before an IDR must not trigger a reset loop
that discards the decoded frame before it can be displayed.

At `decodeQueueSize >= 4`, normal ordered consumption waits for `dequeue`, with a
two-second stall deadline; it does not reset the decoder just because the queue
is full. This preserves references and avoids an unnecessary new-IDR wait on a
static display. Actual decoder errors still use bounded reset/keyframe recovery.
An IDR request needs a producer frame; a static source may require a new frame or
preview restart. The backend requests 100-ms frame repetition, without assuming
every encoder honors that setting.

Keep lease generation and capture/geometry epoch distinct. Version any changed
wire encoding rather than silently repurposing MPP1's generation field. The current
backend stops and requires a fresh stream on rotation instead of dynamically
rebuilding; it stops input and discards callbacks from the old epoch. Include
same-sized rotations and scaled/letterboxed canvases in tests.

Rust must track the active pointer and pressed keys. Release or cancel them on
pointer-capture loss, focus loss, panel closure, control-channel failure, expiry
and geometry change. Reserve queue capacity for release/reset operations and
provide a device-side loss-of-controller watchdog. Initial input covers a single
pointer and explicitly mapped Android keys; arbitrary Unicode/IME, clipboard,
multi-touch, audio and recording remain outside this increment.

### Qualification checklist

1. Test stream/configuration and input state machines: stale epochs,
   malformed framing, overload, edge preservation and uncertain acknowledgements.
2. Prove real capture and decoded display for each allowed API/arm64 target.
   Split encoder configure/surface/start/reset phases, handle output-format
   changes, and verify every error path releases acquired resources.
3. Test the authenticated DSH video route and bounded decoder against slow readers,
   browser abort, plugin unload and reconnect while writes are blocked.
4. Exercise touch/key injection through the independent control worker. Verify visible
   application effects, drag release outside the canvas, rotation and host death.
5. Run ten minutes and ten reconnect cycles in each source-built client. Record
   first-frame time, queue age, CPU and memory. Device PTS and browser clocks are
   not directly comparable; measure local stages or calibrate clocks before
   claiming end-to-end latency. Scope results to the tested device and codec.

The approved Android seam is a small Java `app_process` entry point that initializes
the runtime and loads Rust, `jni` 0.22 for framework access and NDK MediaCodec for
encoding. Tokio networking extends the existing dependency. No scrcpy runtime,
FFmpeg, WebRTC or additional npm package is used by this implementation.

### Recorded qualification evidence

The following streaming/control checks preceded the local sidebar-width extension
and used the unmodified DSH baseline. Auto-fit has its separate, narrowly scoped
visual evidence above.

On the API 32 arm64 emulator, Web decoded H.264 and exercised canvas tap
(Settings → Apps), Back (Apps → Settings), drag scrolling and Home (launcher).
Local visual evidence is `target/dsh/evidence/web-live-control.jpg`. A native
Home-key pair also changed the resumed activity from Settings to NexusLauncher.

A 600.1-second native run remained healthy: first 180 seconds static, then Home-key
pairs every 30 seconds. Recorded output was 1 configuration packet, 8 keyframes,
228 delta frames and 1,184,670 bytes. Native OS checks found no pressed touch at
the 2.6-second watchdog check or after control EOF. Both 90° and 180° rotation
ended the stream as intended; 10/10 fresh start/stop cycles cleaned up, and device
settings were restored. This is a sparse-change native test, not a frontend soak,
frame-rate benchmark, latency measurement or broad reliability qualification.

After fixing normal decode-queue backpressure, Desktop produced its first frame.
Home moved Calendar → launcher; dragging opened the app drawer; taps opened
Settings → Apps; Back returned to Settings. The preview stayed live through sidebar
resizing during about two minutes of interaction. A checked stop left zero MPP
native helpers and owned adb reverse mappings. A subsequent clean Desktop process
restart reproduced a decoded live frame without temporary diagnostics. The final
live-frame screenshot is `target/dsh/evidence/desktop-live-control.png`.
Its competing-preview rejection had also preserved the active capture.

An earlier unexplained Web EOF did not recur during the native run or latest Web
checks; its cause is unresolved. Long end-to-end GUI sessions, additional GUI
restart/reconnect cycles and performance characterization are not established by
these focused checks, which do not close every checklist item.

### API 36 physical-device qualification

The subsequent API 36 adapter passed native capture and GUI/input checks on one
nubia P0110 physical phone (Android 16/API 36, arm64). The `0.1.0-preview.2` archive
was installed into the official stock npm DSH Web `0.2.1-alpha.1` distribution on
macOS arm64. It produced H.264 configuration, key and delta packets and the Web
client decoded live 576×1280 video from the 1264×2800 display. Home returned to the
main launcher page; tapping opened Display & Brightness; dragging scrolled the
settings list; Back returned to the settings main page. Pause left no helper, and
Resume restored Live.

Three separate native start/capture/stop cycles after pausing left no helper,
adb reverse mapping or active MPP virtual display. With the same new host and
native assets, the API 32 emulator still produced configuration/key/delta packets
and cleaned up. The local phone screenshot is
`target/android36/evidence/phone-live.png`. Rust workspace tests (94), host and
Android release Clippy under Rust 1.88, and the package tests (7) passed.

These checks qualify the two tested targets, not every Android 16 device, vendor
or unsupported intermediate API level. Physical rotation, hot unplug and long GUI
sessions were not newly tested in that increment. Its Desktop evidence used a
source build. Subsequent CI-archive installation and official Desktop GUI checks
are recorded in [the release acceptance report](RELEASING.md). No bootstrap change,
dependency or public release was introduced by the API 36 increment.

[scrcpy-capture]: https://github.com/Genymobile/scrcpy/blob/2926c06c5dc3064ae6d8db706f1a98a37cfcf3f0/server/src/main/java/com/genymobile/scrcpy/video/ScreenCapture.java
[scrcpy-encoder]: https://github.com/Genymobile/scrcpy/blob/2926c06c5dc3064ae6d8db706f1a98a37cfcf3f0/server/src/main/java/com/genymobile/scrcpy/video/SurfaceEncoder.java
[scrcpy-frame-buffer]: https://github.com/Genymobile/scrcpy/blob/2926c06c5dc3064ae6d8db706f1a98a37cfcf3f0/app/src/frame_buffer.c
[scrcpy-tunnel]: https://github.com/Genymobile/scrcpy/blob/2926c06c5dc3064ae6d8db706f1a98a37cfcf3f0/app/src/adb/adb_tunnel.c
[scrcpy-connection]: https://github.com/Genymobile/scrcpy/blob/2926c06c5dc3064ae6d8db706f1a98a37cfcf3f0/server/src/main/java/com/genymobile/scrcpy/device/DesktopConnection.java
[scrcpy-controller]: https://github.com/Genymobile/scrcpy/blob/2926c06c5dc3064ae6d8db706f1a98a37cfcf3f0/app/src/controller.c
[scrcpy-droppable]: https://github.com/Genymobile/scrcpy/blob/2926c06c5dc3064ae6d8db706f1a98a37cfcf3f0/app/src/control_msg.c#L359
[scrcpy-position]: https://github.com/Genymobile/scrcpy/blob/2926c06c5dc3064ae6d8db706f1a98a37cfcf3f0/server/src/main/java/com/genymobile/scrcpy/control/PositionMapper.java
[scrcpy-server]: https://github.com/Genymobile/scrcpy/blob/2926c06c5dc3064ae6d8db706f1a98a37cfcf3f0/server/src/main/java/com/genymobile/scrcpy/Server.java
[webcodecs-avc]: https://www.w3.org/TR/webcodecs-avc-codec-registration/
[webcodecs]: https://www.w3.org/TR/webcodecs/

## References

- [scrcpy architecture](https://github.com/Genymobile/scrcpy/blob/v4.1/doc/develop.md)
- [Android MediaCodec](https://developer.android.com/reference/android/media/MediaCodec)
- [DSH Desktop](https://github.com/deepseek-ai/deepseek-harness/blob/master/apps/desktop/README.md)
