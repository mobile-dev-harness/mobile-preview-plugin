# Android compatibility qualification

The recorded archives both use `0.1.0-preview.3` but have different checksums:
the local matrix archive and the downloaded CI archive below. Their arm64 adapter policy
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
preview.3. The subsequent Web API 35–37 and official Desktop API 37 GUI checks used
the downloaded CI archive from run `37652914514`, source commit
`9ec7afad96f486b8050f2f41e6ca8bade24c9b8c`, SHA-256
`d6f6450826638bdb44cce5a2b8669965036f7de87ec8d79e2468d3b5d08f9b9c`.
The same version string does not make these archives interchangeable. Likewise,
the `preview.4` examples in the packaging guide do not inherit
this qualification; a new archive needs its own provenance and acceptance record.

In the table, native matrix results and Web API 30–34 results use the local
archive; Web API 35–37 results use the CI archive. The CI archive separately
passed native API 37 capture/Home and SIGTERM cleanup on the 16 KiB target.

| Android | API | Framework path | Current evidence | Remaining qualification |
| --- | --- | --- | --- | --- |
| 10 | 29 | SurfaceControl / InputManager | Environmentally blocked; capture failed, cleanup passed | A working target and full native/Web validation |
| 11 | 30 | SurfaceControl / InputManager | Native capture/input/cleanup and stock DSH Web GUI passed | Broader target and sustained-session coverage |
| 12 | 31 | SurfaceControl / InputManager | Native capture/input/cleanup and stock DSH Web GUI passed | Broader target and sustained-session coverage |
| 12L | 32 | SurfaceControl / InputManager | Native capture/input/cleanup and stock DSH Web GUI passed | Broader target and sustained-session coverage |
| 13 | 33 | SurfaceControl / InputManager | Native capture/input/cleanup and stock DSH Web GUI passed | Broader target and sustained-session coverage |
| 14 | 34 | DisplayManager / InputManagerGlobal | Native capture/input/cleanup and stock DSH Web GUI passed | Broader target and sustained-session coverage |
| 15 | 35 | DisplayManager / InputManagerGlobal | Local native capture/input/cleanup and CI archive stock Web GUI passed | Broader target and sustained-session coverage |
| 16 | 36 | DisplayManager / InputManagerGlobal | Local native capture/input/cleanup and CI archive stock Web GUI passed; preview.2 phone evidence remains historical | Wider vendor and sustained-session coverage |
| 17 | 37 | DisplayManager / InputManagerGlobal | Local and CI native checks passed with 16,384-byte pages; CI archive stock Web and official Desktop GUI passed | Broader 16 KiB target and sustained-session coverage |

The API 30–34 stock DSH Web checks verified an actual decoded first frame,
dragging from the launcher into the app drawer, tapping Settings, and a visible
page change after Back. The API 34 Back check specifically returned from the Apps
subpage to Settings. Screenshots and `target/android-matrix/gui-results.json`
record these local-archive checks.

The exact CI archive passed stock Web `0.2.1-alpha.1` GUI checks on API 35–37:
decoded first frame, dragging into the app drawer or scrolling, Settings/Apps taps,
Back from Apps to Settings and Home to the launcher. API 37 also kept explicit
Pause across panel reopening, returned to Live with Resume and stayed Live through
a 1280×820/narrow-viewport round trip. These results and screenshots are recorded in
`target/release-readiness/gui/web-matrix.json` and its directory. Web GUI removal
after the connected API 35 session removed the mobile entry, replaced the device
view with DSH's no-renderer placeholder, cleared the dependency/bundle and left no
helper or owned reverse mapping. The panel was hidden before Plugin Manager;
the record is `target/release-readiness/gui/web-ui-remove.json` with screenshot
`web-uninstalled.png` in the same directory. GUI reinstallation, enabling and
reconnecting on API 35 restored a decoded Live view, captured in
`target/release-readiness/gui/web-reinstalled-api35.png`. These are separate GUI observations;
native packet delivery alone does not establish decoding or visible input effects.

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

## Official Desktop installation and GUI evidence

The official macOS arm64 Desktop `0.2.0-rc.2` app passed signature and notarization
checks. Its bundled CLI installed `0.1.0-preview.3` into an isolated profile with
the bundle enabled and native manifest hashes verified. The profile used a
separate webserver port to avoid an existing app instance; no DSH source patch
was required. Evidence is
`target/release-readiness/desktop/evidence/install-preview3.json` and
`target/release-readiness/desktop/install-preview3.log`.

The exact downloaded CI archive subsequently passed GUI checks in this official
Desktop distribution on API 37: decoded first frame, Settings/Apps taps, dragging,
Back/Home, Pause with helper/reverse cleanup, Pause remaining paused after panel
reopening, Resume returning to Live and a native window-zoom round trip retaining
Live. GUI removal removed the plugin entry and renderer and cleared the plugin
dependency, bundle, helper and owned reverse mapping. Opening Plugin Manager hides
the preview, so removal began from a previously connected chat. GUI reinstallation
and enabling restored a decoded live preview. The record and screenshots are in
`target/release-readiness/gui/desktop.json` and its directory.

These checks qualify this archive and API 37 target, not every Desktop/device
combination or extreme interruption path. Remaining scope and review are tracked
in [RELEASING.md](RELEASING.md).

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
separate. The source repository remains private and neither recorded archive is
a public npm or GitHub Release.
