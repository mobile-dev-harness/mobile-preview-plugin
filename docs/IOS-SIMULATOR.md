# Experimental iOS Simulator preview

The current source implements H.264 video and capability-gated single-pointer
tap/drag and Home for a local iOS Simulator. **Native and interactive Web/Desktop
Home, tap and drag effects have passed** on the recorded target. Missing native
input capability leaves a read-only preview. The previous read-only phase passed
local decoded-video checks in
stock DSH Web `0.2.1-alpha.1` and official Desktop `0.2.0-rc.2` on macOS 26.7 Apple
Silicon / Xcode 26.4 / iOS 26.4 / iPhone 17. These scoped passes leave several
lifecycle checks open, as recorded below. The exact `0.1.0-preview.5` archive now
has its own scoped qualification record below; earlier Android archives do not
inherit this iOS scope.

## Published preview.5 qualification

[**GitHub prerelease `v0.1.0-preview.5`**](https://github.com/mobile-dev-harness/mobile-preview-plugin/releases/tag/v0.1.0-preview.5)
is published, with the [archive](https://github.com/mobile-dev-harness/mobile-preview-plugin/releases/download/v0.1.0-preview.5/mobile-dev-harness-dsh-mobile-preview-0.1.0-preview.5-darwin-arm64.tgz),
[checksum](https://github.com/mobile-dev-harness/mobile-preview-plugin/releases/download/v0.1.0-preview.5/mobile-dev-harness-dsh-mobile-preview-0.1.0-preview.5-darwin-arm64.tgz.sha256)
and [qualification JSON](https://github.com/mobile-dev-harness/mobile-preview-plugin/releases/download/v0.1.0-preview.5/qualification-0.1.0-preview.5.json).
The iOS scope is local Simulator on the recorded macOS Apple Silicon / Xcode 26.4 /
iOS 26.4 / iPhone 17 target. Physical iOS is excluded. CI, clean provenance and the
scoped exact-archive checks passed. The [release record](RELEASING.md) records
publication, checksums and remaining limitations.

The immutable archive comes from commit
`31a2bdb1c4d96345c7735377d6d861fe52ce12a4`, which remains the release-tag target.
Archive SHA-256 is
`04cd7427bdb45bb00d823d789a2573969711a93cee1da837132520703f59f2af`;
the bundled host SHA-256 is
`aab0a241a91b5903bba932cf3114172739fe441ca1c69e7025da971892c0dcc9`.
The source-tree hash, successful six-job CI and packaging runs are pinned in the
release checklist. This documentation follow-up reports that unchanged archive;
older binary hashes/counts below remain attached to their development increments.

| Exact preview.5 archive check | Result |
| --- | --- |
| Fresh stock Web 0.2.1-alpha.1 | Passed: plugin UI booted the selected stopped Simulator, then Connect automatically decoded the first frame |
| Web input and viewport | Passed: Settings tap, visible scrolling, Home, bottom Home swipe and accurate tap at 800×900 after resizing |
| Web Pause/reopen/Resume | Passed; Pause survived panel reopening and its capture child was reaped |
| Fresh official Desktop 0.2.0-rc.2 | Passed: first frame, tap/drag/Home/bottom Home swipe, Pause/Resume and disconnect |
| Native STOP, stdin EOF and SIGTERM | Passed: exit 0, no forced termination and owned sockets removed |
| Actual Simulator reboot | Passed: old lease became `STALE_SESSION`; reconnection used a new boot identity and new generation |
| CI | All six jobs passed; macOS Rust 153 passed with one device probe ignored, plugin 185 passed, scripts 46 passed; format, Clippy and MSRV 1.88 passed |
| Web GUI removal/reinstallation | Passed: after Plugin Manager hid the connected preview, removal cleared dependency/bundle/link and owned host/capture; local `.tgz` Add and Enable Now restored matching native hashes, then Connect reached Live |
| Final native App Switcher | Passed: the CI-built host's timed bottom swipe/hold produced actual cards observed through freshly reinstalled Web |
| Live orientation/geometry rejection and manual Desktop App Switcher | Unqualified |

The Desktop run used `--force-renderer-accessibility` only for automated accessibility
inspection, not a media or application-code change. A separate exact-archive
Android API 35 arm64 five-second stream/cleanup smoke passed without input.
The final [reinstalled-Web App Switcher view](../target/release-simulator/evidence/web-reinstalled-app-switcher.png)
records the observed result; the published qualification JSON retains release evidence.

Web and Desktop source checks cover video, single-pointer tap/drag, Home and bottom
Home swipe. The earlier manual Web App Switcher gesture passed on 2026-10-10;
manual Desktop App Switcher remains unqualified. The exact-archive timed native
check above is separate from mouse-operated UI qualification. Missing input capability retains
read-only video. Fixed portrait geometry, private CoreSimulator interfaces and
gesture-end cleanup limitations remain part of the release scope.

## Scope and setup

Use a macOS Apple Silicon Host with Xcode, an installed iOS Simulator runtime and
an existing Simulator. The initial capture scope is upright portrait only.
Orientation, display geometry or source surface changes terminate the current
stream. Return the Simulator to upright portrait before starting another preview.
Other Xcode/runtime combinations are unqualified; private CoreSimulator interfaces
are checked at runtime and may fail on a different installation.

Input-capable previews support one pointer for tap/drag and a Home button.
Keyboard/text/IME, multi-touch, Back, clipboard, audio, recording and physical iOS
devices are unsupported. Use Apple's Simulator app for unsupported interactions.
The agent tool only requests the platform panel; it supplies neither device control
nor model vision.

For the Home gesture, start inside the bottom 2% of the actual screen and make a
short upward swipe. The decorative bezel is outside the touch surface. The bottom
edge is chosen from the initial touch and remains fixed throughout that gesture.
App Switcher has passed both a timed protocol gesture and a user-operated mouse
gesture in Web. The manual Desktop App Switcher gesture remains unverified.

Ending or cleaning up a held iOS gesture can complete its tap or drag. The native
service exposes gesture end, not UIKit `touchesCancelled`; the preview cannot
promise to cancel a held gesture without activating its current target.

Build the source with the repository's Rust toolchain, then list available devices:

```sh
cargo build --workspace --locked
./target/debug/mpp devices
```

Find the exact UDID in an `ios:<UDID>` record. Connecting does not boot a stopped
Simulator. Startup is a separate explicit user action:

```sh
./target/debug/mpp boot --simulator <UDID>
./target/debug/mpp probe --simulator <UDID>
```

The CLI uses `/usr/bin/xcrun` by default and accepts an absolute override with
`--xcrun /absolute/path/to/xcrun`. Xcode's selected developer directory determines
the active toolchain and runtimes. The DSH plugin's host configuration also accepts
an absolute `xcrun` path. Neither discovery nor connection installs Xcode/runtimes,
creates a Simulator or picks one implicitly. A successful startup remains running
after preview disconnect or Host exit.

In DSH, choose **iOS** from the platform landing view, select a Simulator, explicitly
start it if needed, and choose **Connect**. The canvas uses the existing WebCodecs
decoder and must receive a decoded frame before it reports Live. Pause/Resume and
visibility cleanup follow the existing preview lifecycle. Node enables touch and
Home only when the native probe reports `Device.capabilities.input: true` after
a positive DTUHID no-event handshake. If that capability is absent, it reports
`input: false` and keeps read-only video. Platform availability alone never enables
input; keyboard and Back remain unavailable in either case.
An agent request such as `open_mobile_preview({platform: "ios"})` opens this
selection flow for its calling chat; the user still chooses and connects the device.

This backend does not need adb, an Android SDK, `bootstrap.jar` or
`libmpp_android_device.so`. The combined source/package can retain Android assets
for Android use. Its iOS backend uses Apple components already installed with
macOS and Xcode; their binaries are not redistributed.

## Native capture and transport

`mpp-ios` adds no external dependency. Discovery and explicit startup use `xcrun
simctl`; capture uses direct Objective-C runtime FFI into the installed private
CoreSimulator framework. Runtime capability checks validate the classes,
protocols and selectors used to acquire the display's IOSurface. The current
development machine has CoreSimulator **1174.9.2** installed; that observation is
not a promise of compatibility with every Xcode or CoreSimulator release.

The native capture path is:

1. Acquire the shared raw IOSurface and sample its pixels at every configured,
   bounded frame interval. GPU-rendered changes do not reliably advance its seed,
   so sampling never waits for a seed change. This is not a PNG or screenshot loop.
2. Copy and scale into an owned NV12 pixel buffer with `VTPixelTransferSession`.
   Use before/after seed checks as an advisory check for concurrent writes, dropping
   a copy when the seed changes. An unchanged seed does not prove GPU inactivity.
3. Encode H.264 through `VTCompressionSession`, convert encoded access units into
   the existing MPP1 framing, and forward them to the existing WebCodecs client.

The browser preserves its active decoder and pending canvas frame when identical
SPS/PPS configuration is repeated. Repeating configuration before an IDR therefore
does not continually reset the decoder and discard a frame before display.

The current `mpp` executable isolates this work in a private `--ios-capture`
child. A full-duplex Unix socket is inherited as FD 0 with `--control-fd 0`, and
`--enable-input` is passed only for a device whose input capability was verified.
Stdout remains bounded MPP1 output and stderr remains diagnostics. The browser
continues to use authenticated DSH routes and host-owned Unix sockets. The preview
descriptor retains exactly
`{stream_id, epoch, generation, geometry, video_socket, control_socket}`.
The control path carries capture commands and capability-gated input; the socket's
presence alone does not imply that input is enabled.

On CoreSimulator 1174.9.2, the legacy Indigo route suppresses input. MPP uses the
DTUHID digitizer XPC service through a device-specific Mach port instead. A positive
no-event handshake establishes capability without injecting a touch or Home press.
Actual events use `isBarrier: false`, followed by an acknowledged no-event barrier.
HTTP success waits for the actual native reply; it confirms ordered submission,
not application-level completion. Native effect checks therefore observe the
Simulator screen separately from protocol receipts.

Input validates the active boot identity, frame geometry, sequence and capture
epoch, and rechecks timed-out work before injection. Its wakeup does not depend on
the video frame rate. A two-second held-input watchdog, reset, EOF and native owner
drop release owned holds. DTUHID maps `cancel` to end, so this cleanup may complete
the current gesture; it does not synthesize UIKit cancellation.

A lease binds the exact Simulator boot, using its UDID, `launchd_sim` PID and
libproc process start time. The child verifies that identity at startup and
periodically. A reused UDID after reboot cannot keep the old capture alive.
EOF, stop and SIGTERM paths release owned surfaces, encoder state, pipes and
child resources without shutting down the user's Simulator. The qualification
table distinguishes checked cleanup paths from live cases that remain open.

## Current input qualification

This section records the pre-packaging input-feature baseline. The preview.5
archive's newer checks are in the published-preview section above.

The current input checks used macOS **26.7** Apple Silicon, Xcode **26.4 (17E192)**,
iOS **26.4**, iPhone **17**, CoreSimulator **1174.9.2**, and Simulator UDID
`D228B7F4-088B-40F8-ACA6-2083A2810F43`. The source remains an uncommitted dirty
checkout based on `937e5bb`, not a published package. The input-feature baseline
binary SHA-256, before the bottom-edge fix below,
is `aa0c0321691fe7a870b1b7c6b0646f6cd92cb76ce2e6a583232905630fe060ae`.
Its local evidence lives under `target/ios-input/`; the final evidence record keeps
the source-tree identity outside these tracked docs.

Stock Web **0.2.1-alpha.1** and official Desktop **0.2.0-rc.2** both returned to
SpringBoard with Home, opened Settings/General by tapping the Settings icon, and
visibly scrolled the list by dragging. Web additionally tapped General accurately
after changing the viewport to **800×900**, moving and resizing the canvas; the
baseline layout also passed. Native Home/tap/drag effects were independently
observed. These observations establish app effects for the tested actions,
separately from DTUHID barrier acknowledgements.

| Current input check | Status |
| --- | --- |
| Native Home | Passed: actual Home effect observed |
| Native single-pointer tap | Passed: actual Settings navigation observed |
| Native single-pointer drag | Passed: actual Settings scrolling observed |
| Interactive stock Web `0.2.1-alpha.1` Home/tap/drag | Passed: Home → SpringBoard, canvas tap → Settings/General, drag → visibly scrolled list |
| Interactive official Desktop `0.2.0-rc.2` Home/tap/drag | Passed: Home → SpringBoard, canvas tap → Settings/General, drag → visibly scrolled list |
| Web input after canvas movement/resizing | Passed: General tapped accurately at an 800×900 viewport; baseline layout also passed |
| Read-only fallback when native input probing fails | Implemented; preserve separate qualification evidence |
| Rust workspace | 149 passed, one device probe ignored by default; the native Home probe was explicitly run and its effect verified |
| Plugin and script tests | 185 plugin tests and 46 script tests passed |
| Format, Clippy and MSRV | Passed, including Rust 1.88 |
| Android API 35 regression with current input build | Passed for five seconds without input: native stream and cleanup |
| UI boot, live rotation/geometry rejection, actual reboot invalidation and full plugin uninstall | Not live-qualified for this source baseline; newer exact-archive boot/reboot results are recorded above |
| Other Xcode/runtime versions | Unqualified |

Local native evidence: [Settings after tap](../target/ios-input/evidence/settings-after-native-tap.png),
[Settings after drag](../target/ios-input/evidence/settings-after-native-drag.png) and
[native drag record](../target/ios-input/evidence/native-drag.json).

Web evidence: [Home](../target/ios-input/evidence/web-home.png),
[Settings tap](../target/ios-input/evidence/web-tap-settings.png),
[visible drag scrolling](../target/ios-input/evidence/web-drag.png), and
[General tap after resizing](../target/ios-input/evidence/web-scaled-tap-general.png).
Desktop evidence: [Home](../target/ios-input/evidence/desktop-home.png),
[Settings tap](../target/ios-input/evidence/desktop-tap-settings.png), and
[visible drag scrolling](../target/ios-input/evidence/desktop-drag.png).
The [Android regression](../target/ios-input/android-regression.json) used no input
and does not renew Android input qualification. These results do not establish
touch-cancellation semantics or close the untested lifecycle cases above.

### Bottom-edge gesture increment

The subsequent edge fix uses binary SHA-256
`d7b892e2526b2544a89f3e6e75817210a42e9d992da56e5a3b7d79ffcc3e1942` on the same
local target. A touch beginning at normalized `y >= 0.98` uses bottom-edge value
`3`, latched from down through move, up, cancel and owned-hold release. This fixes
bottom gestures previously submitted as ordinary in-screen touches. The existing
Cancel → End limitation still applies.

| Edge-increment check | Status |
| --- | --- |
| Web short bottom swipe | Passed: Settings → SpringBoard |
| Timed control-protocol swipe and pause | Passed: `y=0.995` → `0.55` over 12 moves, hold 650 ms, then up; independent Simulator observation showed Settings and Maps cards in the app switcher |
| Web manual App Switcher gesture | Passed on 2026-10-10: user reported success from Home; the preview screenshot showed actual App Switcher cards and the trace contained completed touch sequences without errors |
| Desktop manual App Switcher gesture | Not verified; the UI automation drag does not expose a hold |
| Desktop short bottom swipe | Passed: Settings → SpringBoard |
| Interior-drag recheck | Web visibly scrolled Settings to Password, Privacy, Game Center and Developer without triggering Home; Desktop stayed in General at its scroll limit |
| Targeted `mpp-ios` tests | 51 passed: 33 unit and 18 integration; one device probe ignored |
| Format, workspace Clippy and MSRV 1.88 | Passed |

Evidence: [Web bottom swipe](../target/ios-edge/evidence/web-bottom-swipe.png),
[Desktop bottom swipe](../target/ios-edge/evidence/desktop-bottom-swipe.png),
[Web before ordinary scroll](../target/ios-edge/evidence/web-interior-before.png),
[Web after ordinary scroll](../target/ios-edge/evidence/web-interior-after.png),
[independent Simulator app-switcher view](../target/ios-edge/evidence/simulator-app-switcher.png),
[timed protocol record](../target/ios-edge/evidence/switcher-protocol.json), and
[user-operated Web App Switcher](../target/ios-home-gesture/evidence/user-web-app-switcher.png).
The manual Web check used the same edge-fix binary; its temporary host timing
preload was then removed. No additional gesture code was changed for that check.
The full Rust workspace suite was not rerun for this increment. The earlier 149
Rust, 185 plugin and 46 script passes remain baseline evidence, not new edge-fix
test totals. This is an uncommitted source change, not a published package.

## Previous phase: read-only video qualification

The following records belong to the earlier read-only implementation. They remain
video/lifecycle evidence for that build, not input qualification for the current
capability-gated implementation.

The local checks used macOS **26.7**, Apple Silicon, Xcode **26.4 (17E192)**,
iOS **26.4**, iPhone **17**, Simulator UDID
`D228B7F4-088B-40F8-ACA6-2083A2810F43`, and installed CoreSimulator **1174.9.2**.
The source was a local dirty checkout based on `937e5bb`, not a released package.
The read-only phase's final native binary SHA-256 is
`0c69687d54ad974af144e3ad9ffe27f8b5c04052cb0bda1ab051d020a7709634`.
Local evidence is under `target/ios-preview/`; the evidence summary records the
dirty source-tree identity separately from these tracked documents.

| Target or check | Status |
| --- | --- |
| Stock Web `0.2.1-alpha.1` decoded video | Passed: actual Settings screen reached Live and changed between light/dark appearances |
| Web revalidation with the final binary above | Passed: decoded Live video in the original local development preview |
| Official Desktop `0.2.0-rc.2` decoded video | Passed: actual Settings screen reached Live and changed between light/dark appearances |
| Read-only UI | Passed in both clients: Home/Back hidden and device input unavailable |
| Device-input rejection | Unit-tested; no actual iOS input was injected |
| Web Pause/Resume and paused panel reopening | Passed: Pause retained across reopening, Resume returned to video, stopped capture left zero helpers |
| Desktop Pause | Passed |
| Desktop Resume | Passed: returned to Live with the final binary above |
| Final native full-host smoke | Passed over 10 seconds: 6 configuration packets, 6 keyframes and 278 delta frames; keyframe/heartbeat/reset control, stop, disconnect and clean EOF |
| Native capture cadence | Passed bounded-cadence checks described below; not an end-to-end performance claim |
| Capture-child EOF, SIGTERM, backpressure and mismatched boot ID | Passed |
| Rust workspace tests | 121 passed, including the SDK-compatible packed-4 `CMTime` layout regression |
| Android API 35 native regression | Passed for five seconds without device input: stream, stop, disconnect, host EOF and owned-resource cleanup |
| Explicit Simulator startup through the plugin UI | Not yet live-qualified |
| Orientation/geometry rejection | Not yet live-qualified |
| Actual Simulator reboot invalidating its old capture/session | Not yet live-qualified; mismatched boot-ID check is narrower evidence |
| Full plugin uninstall with iOS capture | Not yet live-qualified |
| Other Xcode versions, iOS runtimes and Simulator types | Unqualified |
| Input in this phase | Not implemented; superseded by the capability-gated input phase above |
| Physical iOS devices | Not implemented |

The [final full-host smoke result](../target/ios-preview/smoke.json) records the
10-second run with 6 configuration packets, 6 keyframes and 278 delta frames,
followed by clean stop, disconnect and EOF. The earlier
[30-second smoke result](../target/ios-preview/smoke-initial-idr.json) predates the
cadence correction and produced IDR-heavy output; it proves the recorded lifecycle
and control checks, not frame cadence or performance. A separate earlier
post-correction eight-second native capture with a 30 fps ceiling produced **4 IDR and 197 delta
frames**, about **25.1 fps observed**. A separate 5 fps ceiling check had a minimum
presentation-timestamp spacing of **201,289 microseconds**. These are short native
capture observations, not browser frame-rate, latency or sustained-throughput
guarantees. The [Android regression result](../target/ios-preview/android-regression.json)
records the separate API 35 `emulator-5570` five-second stream/cleanup check; no
Android input was requested, so it does not renew input qualification.

Local GUI evidence includes [Web first frame](../target/ios-preview/evidence/web-first-frame.png),
[Web resumed video](../target/ios-preview/evidence/web-resumed.png) and
[Desktop changing frame](../target/ios-preview/evidence/desktop-changing-frame.png).
The [final Desktop Live frame](../target/ios-preview/evidence/desktop-final-live.png)
records the confirmed Resume result with the final binary above. The
[final Web Live frame](../target/ios-preview/evidence/web-final-live.png) records
revalidation of that binary in the original development preview.
These ignored local files are evidence for this checkout, not published artifacts.
Retain failures as well as retries, and update open rows only with their own scoped
evidence. One Simulator's result does not qualify physical devices, every runtime
or sustained GUI reliability.

## Source references and dependency boundary

Apple's [VideoToolbox documentation](https://developer.apple.com/documentation/videotoolbox)
describes the system encoding and pixel-transfer APIs used by MPP. Public Apple
frameworks are used from the installed system. CoreSimulator acquisition uses
private runtime interfaces, which have a separate compatibility risk from those
public encoding APIs.

[facebook/idb at `c9ec2501362d97b775d818db8660fdee145f32ea`](https://github.com/facebook/idb/tree/c9ec2501362d97b775d818db8660fdee145f32ea)
was inspected as a reference for CoreSimulator and DTUHID protocols. MPP does not vendor,
launch, link or require idb/FBSimulatorControl, and no idb runtime is shipped.
The implementation uses its own Rust FFI and the installed Apple frameworks.
