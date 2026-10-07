# Android compatibility qualification

The recorded matrix candidate is `0.1.0-preview.3`. Its implemented arm64 adapter policy
covers Android 10–17 / API 29–37. Eligibility means a matching capture/input path
exists; it does not mean that every version, emulator image or vendor device has
passed runtime qualification. APIs outside 29–37 and non-arm64 devices are rejected.

## Implemented policy and build baseline

The shared `android_framework(api)` policy selects `SurfaceControl` capture with
`InputManager` input on APIs 29–33, and `DisplayManager` mirroring with
`InputManagerGlobal` input on APIs 34–37. Host and native checks use the same policy.
The tiny Java bootstrap is unchanged; capture, input and cleanup remain in Rust.

NDK and DEX outputs target minimum API 29. Device assets are now packaged under
`native/android-arm64`. `runtime-manifest.json` records `minApi: 29` and the
implemented API list 29–37; `supportedApis` is adapter metadata, not a test-pass list.

The build applies 16 KiB linker alignment and verifies each ELF `PT_LOAD` segment.
The inspected native library had four LOAD segments, all aligned to `0x4000`.
This proves an artifact property; runtime page-size qualification is recorded
separately. The API 37 emulator below has now passed native capture/input/cleanup
with an observed 16,384-byte page size. That single run does not qualify every
16 KiB device. See the [Android alignment guidance](https://developer.android.com/guide/practices/page-sizes#compile-16-kb-alignment).

## Runtime evidence matrix

Results are scoped to the tested target and archive. The consolidated local record
is `target/android-matrix/verification.json`, recorded on 2026-10-07. It identifies
`0.1.0-preview.3` with archive SHA-256
`6e7da3cc75301de0a405aa7bbe058055c6d5ff3cfc64b2a8c5d01d33062c418b`, and records
that its native assets and JavaScript runtime match the tested candidate. The
preview.1/preview.2 records remain historical and are not automatically passes for
preview.3. Likewise, the `preview.4` examples in the packaging guide do not inherit
this qualification; a new archive needs its own provenance and acceptance record.

| Android | API | Framework path | Current evidence | Remaining qualification |
| --- | --- | --- | --- | --- |
| 10 | 29 | SurfaceControl / InputManager | Environmentally blocked; capture failed, cleanup passed | A working target and full native/Web validation |
| 11 | 30 | SurfaceControl / InputManager | Native capture/input/cleanup and stock DSH Web GUI passed | Broader target and sustained-session coverage |
| 12 | 31 | SurfaceControl / InputManager | Native capture/input/cleanup and stock DSH Web GUI passed | Broader target and sustained-session coverage |
| 12L | 32 | SurfaceControl / InputManager | Native capture/input/cleanup and stock DSH Web GUI passed | Broader target and sustained-session coverage |
| 13 | 33 | SurfaceControl / InputManager | Native capture/input/cleanup and stock DSH Web GUI passed | Broader target and sustained-session coverage |
| 14 | 34 | DisplayManager / InputManagerGlobal | Native capture/input/cleanup and stock DSH Web GUI passed | Broader target and sustained-session coverage |
| 15 | 35 | DisplayManager / InputManagerGlobal | Native capture/input/cleanup passed | GUI not yet verified |
| 16 | 36 | DisplayManager / InputManagerGlobal | Native capture/input/cleanup passed; historical preview.2 phone GUI evidence retained separately | Candidate GUI not yet verified; wider vendor coverage |
| 17 | 37 | DisplayManager / InputManagerGlobal | Native capture/input/cleanup passed on Android 17 with 16,384-byte pages | GUI not yet verified |

The API 30–34 stock DSH Web checks verified an actual decoded first frame,
dragging from the launcher into the app drawer, tapping Settings, and a visible
page change after Back. The API 34 Back check specifically returned from the Apps
subpage to Settings. Screenshots and `target/android-matrix/gui-results.json`
record these checks. API 35–37 GUI checks remain unverified; a native pass does not substitute
for browser decoding or visible input effects. Browser control became unavailable
after API 34 verification; these pending checks are not recorded as GUI failures.

The API 37 native report (`target/android-matrix/api37-native.json`) records one
configuration packet, three keyframes and 86 delta frames, a verified transition
from a non-launcher app to the launcher after Home, and successful preview stop,
session disconnect, normal host exit and removal of the owned helper/reverse
mapping. Packet counts and first-key timing are smoke-test observations, not a
sustained frame-rate or end-to-end latency benchmark.

The API 29 investigation used Google APIs r13 and AOSP default r8 arm64 images
with emulator versions 35.6 and 37.2. The default Codec2 surface path failed with
zero-sized BufferQueue dimensions followed by a 1×1 surface. scrcpy 4.1 and the
system `screenrecord` also failed independently in this environment. That evidence
points to a local image/emulator codec limitation; it neither establishes an MPP
runtime pass nor proves all Android 10 devices are broken. Temporary OMX-selection
and Rust startup-order trials were reverted. No workaround is shipped in MPP.

A final control used the AOSP API 29 r8 image in an isolated Emulator 37.2.12 with
host GLES and Vulkan disabled, both confirmed by startup logs. It still failed:
system `screenrecord` returned `err=-38` and an empty output, while MPP reported
`GraphicBufferSource` BufferQueue dimensions `0x0`/error `-22`,
`C2SoftAvcEnc` capacity `1x1`, and `setEncodeArgs` error `22`. No video packets
arrived; owned helper/tunnel/session cleanup succeeded. Thus switching away from
SwiftShader and disabling Vulkan did not unblock this local target. API 29 remains
unverified for capture/input, not passed with a known warning.

Local evidence files are `target/android-matrix/api29-host-gpu-native.json`,
`target/android-matrix/api29-host-gpu-codec.log`, and
`target/android-matrix/api29-host-gpu-screenrecord.json`.

The API 32 historical record includes decoded video and input in source Web/Desktop,
stock Web package installation/removal, and native lifecycle checks. The API 36
historical record covers one nubia P0110 on stock DSH Web: decoded video, tap/drag,
Home/Back, Pause/Resume, and three native capture/cleanup cycles. Neither record
qualifies other vendors or stock Desktop GUI behavior. Phone rotation, hot unplug
and long GUI sessions remain separate qualification work.

## Official Desktop installation evidence

The official macOS arm64 Desktop `0.2.0-rc.2` app passed signature and notarization
checks. Its bundled CLI installed `0.1.0-preview.3` into an isolated profile with
the bundle enabled and native manifest hashes verified. The profile used a
separate webserver port to avoid an existing app instance; no DSH source patch
was required. Evidence is
`target/release-readiness/desktop/evidence/install-preview3.json` and
`target/release-readiness/desktop/install-preview3.log`.

GUI control was unavailable during that check. First decoded frame, visible
tap/drag/Home/Back effects, Pause/Resume, panel reopening and removal cleanup on
this official Desktop distribution remain unverified. The installation check
and earlier source Desktop GUI checks do not close those gaps. Remaining release
acceptance is tracked in [RELEASING.md](RELEASING.md).

## Reproduce native smoke checks

Build the host and Android assets first. Use an already running, authorized device
and its exact serial; the script does not download SDK packages, boot an emulator
or choose a device. Before an input-enabled run, put a non-launcher app such as
Settings in the foreground. The command below sends Home down/up through MPP and
checks the transition to the launcher, so run it only on a device whose foreground
may be changed:

```sh
python3 -B scripts/smoke-android-stream.py \
  --serial emulator-5556 \
  --executable /absolute/path/to/mpp \
  --device-assets /absolute/path/to/android-device \
  --output /absolute/path/to/evidence.json \
  --duration 5
```

Replace the example serial explicitly. Add `--no-input` to skip Home injection and
launcher verification, or `--adb /absolute/path/to/adb` to select the SDK tool.
No-input results do not qualify Home behavior. An input-enabled run rejects an
initially foreground launcher or an unconfirmed initial app rather than claiming
a Home effect from an unchanged launcher. An incomplete MPP1 packet has one shared
deadline spanning its header and payload.
The runner checks bounded MPP1 framing, configuration/key packets, generation and
geometry, then attempts preview stop, session disconnect and host shutdown. It
checks removal of its owned helper process and adb reverse mapping. The JSON report
contains aggregate packet counts, device API/ABI/page size, geometry, input results
and cleanup metadata; it does not retain video, device serial, tokens or raw device
content. A successful native check does not prove browser decoding or rendering.

Use stock DSH Web for the separate GUI check. After installing a new native package
version, restart the DSH Host before connecting: hot reload can retain an old
package-relative asset path. Keep the previous and candidate package evidence
separate. The candidate remains a private local archive, not a public release.
