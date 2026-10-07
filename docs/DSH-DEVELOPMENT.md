# DSH source development

The MPP adapter is a local external plugin for DSH Web and Desktop. Its JavaScript
host/UI layer delegates Android discovery, capture and input to Rust. It implements
a connection panel, H.264/WebCodecs preview, single-pointer interaction and basic
Android keys. The current candidate implements **API 29–37 arm64-v8a Android
adapters on a Unix host**. The shared host/device policy selects the API 29–33
SurfaceControl/InputManager path or API 34–37 DisplayManager/InputManagerGlobal
path. Implementation eligibility is separate from runtime qualification; consult
the [Android matrix](ANDROID-COMPATIBILITY.md). Historical target-emulator and
API 36 phone evidence is retained below. Broader platform, long GUI-session and
performance qualification remain outstanding.
It provides no agent screenshot tools and requires no mdh installation.
The adapter has no third-party Node dependencies and uses DSH's runtime-provided
React/UI services. Lifecycle operations use an internal Rust stdio protocol;
media and input use dedicated private sockets through authenticated DSH routes.
The plugin registers no MCP or agent tools.

## Build a self-contained preview package

The local packaging path targets a macOS 14 or later Apple Silicon DSH Host with Android
API 29–37 arm64 adapters. It does not require the sidebar-width patch. Installation
from the resulting archive does not compile or download MPP native code.

Build on macOS arm64 with Node/npm, Rust **1.88.0**, Android NDK **26.1.10909125**,
the installed `aarch64-apple-darwin` and `aarch64-linux-android` Rust targets, and
the JDK/Build Tools prerequisites in [Build the Android assets](#build-the-android-assets).
Run from this repository:

```sh
RUSTUP_TOOLCHAIN=1.88 node scripts/package-preview.mjs --version 0.1.0-preview.4
```

The script builds the release host and release Android assets with locked Cargo
dependencies into `target/preview-build`, stages only the plugin's runtime files,
and runs `npm pack` with scripts disabled. It installs no toolchains. Cargo may
fetch uncached dependencies; set `CARGO_NET_OFFLINE=true` when everything is cached.
Installed toolchain selection uses the same environment overrides as the Android
build. The Rust/NDK versions must match `licenses/third-party/manifest.json`; the
packager rejects version drift rather than carrying notices for another toolchain.
`MPP_BUILD_PROFILE=release` is selected by the packager itself.

The command above creates:

```text
target/packages/mobile-dev-harness-dsh-mobile-preview-0.1.0-preview.4-darwin-arm64.tgz
target/packages/mobile-dev-harness-dsh-mobile-preview-0.1.0-preview.4-darwin-arm64.tgz.sha256
```

Use `--output-dir PATH` to select a destination. `--version` is required and has no
default. `0.1.0-preview.4` is only a development example, not a published or
qualified release. Versions must use the source package version followed by
`-preview.<number>`. Existing archive/checksum files are not overwritten; choose
another preview version or output directory. The Rust host version must match
the source plugin version. The staged package remains `private: true`.

The archive contains the JavaScript adapter, package README, default DSH patch,
Apache-2.0 `LICENSE`, `THIRD-PARTY-NOTICES.md` and `licenses/third-party/`, plus
these native runtime files:

```text
native/darwin-arm64/mpp
native/android-arm64/bootstrap.jar
native/android-arm64/libmpp_android_device.so
runtime-manifest.json
```

The runtime manifest records the source/package versions, source commit, dirty
state and source-tree SHA-256 (`source.commit`, `source.dirty`, `source.treeSha256`),
supported host/device target, release build profiles, pinned DSH baseline and
SHA-256 hashes of the native files. Packaging rejects source changes during the
build. It also checks `Cargo.lock` and each inventoried notice-file hash against
the third-party license manifest. Refresh the inventory and notices when changing
dependencies or toolchains. Its Android metadata uses `minApi: 29` and
`supportedApis` from 29 through 37; this records adapter eligibility, not runtime
test results. Assets now use the version-neutral `native/android-arm64` directory.
The adjacent `.sha256` records the archive checksum. These are integrity records,
not signatures. The package resolves its executable and assets relative
to its own installation location; `MPP_EXECUTABLE` is not required. Explicit
absolute `executable` and `deviceAssets` configuration remains available for
development. A source checkout without bundled native files needs that explicit
configuration, as supplied by `dev-dsh.mjs`.

To install, use DSH **Plugins → Add plugin** with the archive's absolute path,
then enable the plugin. For the Web CLI profile:

```sh
dsh plugin --profile web add /absolute/path/mobile-dev-harness-dsh-mobile-preview-0.1.0-preview.4-darwin-arm64.tgz
```

The runtime machine needs DSH, Android SDK platform-tools and an authorized API
29–37 arm64 device; check the matrix for its current qualification status. Emulator
use also requires the emulator package and an existing
supported AVD. It does not need Rust, NDK or JDK. Web playback
requires a supported H.264 WebCodecs browser. The source launcher and its Node/pnpm
requirements below are a separate development workflow. Follow the
[package README](../packages/dsh-plugin/README.md) to connect and remove the plugin.
Stock DSH keeps manual sidebar sizing; the optional patch adds automatic width.

Restart the DSH Host after installing a new native package version and before
connecting a device. In particular, changing the installed native asset directory
requires a restart: hot reload can retain the previous package-relative asset path.

The earlier approximately 1.4 MiB `0.1.0-preview.1` archive passed clean installation on
the official npm DSH `0.2.1-alpha.1` Web distribution in a fresh, isolated
`DSH_HOME`. CLI installation used no `--patch`, executable/assets overrides or
model API key. Web decoded an automatic first frame; dragging opened the app
drawer, taps opened Settings → Apps, Back returned to Settings and Home reached
the launcher. At a 1280×820 viewport, manually resizing the sidebar from 300 to
430 pixels kept the preview live. Pause remained paused across collapse/reopen,
and Resume returned to Live.

CLI removal while connected removed the mobile entry and unloaded preview. No MPP
host, Android bootstrap process or adb reverse mapping remained, and the clean
profile no longer contained the plugin bundle or dependency. Local screenshot:
`target/stock-validation/evidence/stock-web-live.png`. These checks qualify the
packed Web path on the target emulator. The Desktop GUI evidence later in this
guide is from a source build.

The official Desktop `0.2.0-rc.2` app later passed signature and notarization
checks. Its bundled CLI installed `0.1.0-preview.3` into an isolated profile;
the bundle was enabled and its native files matched the archive manifest hashes.
The profile used a separate webserver port to avoid a conflict with the existing
app instance. This was a test configuration change, not a DSH source patch.
Evidence is `target/release-readiness/desktop/evidence/install-preview3.json` and
`target/release-readiness/desktop/install-preview3.log`. Official Desktop GUI
preview/input/lifecycle acceptance remains pending because GUI control was
unavailable during that check.

The preview.1 packaging increment passed 143 plugin tests and 19 script tests,
including package relocation and concurrent packaging. Rust formatting and Clippy
passed; the real release host and Android assets built with Rust 1.88 and JDK 17.
These are historical checks, distinct from the source-based runtime checks below.

The earlier `0.1.0-preview.2` archive was installed into the official stock
npm DSH Web `0.2.1-alpha.1` distribution on macOS arm64. On one nubia P0110 phone
(Android 16/API 36, arm64), native capture produced configuration, key and delta
packets, and the Web client decoded live video at 576×1280 from the 1264×2800
display. Home returned a secondary launcher page to the main page; a canvas tap
opened Display & Brightness, dragging scrolled the settings list, and Back returned
to the settings main page. Pause left no helper process, and Resume returned to Live.

After pausing, three independent native start/capture/stop cycles passed and left
no helper, adb reverse mapping or active MPP virtual display. The same new host and
native assets passed an API 32 emulator regression with configuration/key/delta
packets and cleanup. Local screenshot: `target/android36/evidence/phone-live.png`.
This evidence covers these two targets; it does not qualify other Android 16
devices or vendors. Physical rotation, hot unplug and long GUI sessions were not
newly tested. Official Desktop GUI acceptance remains pending.

The API 36 change passed 94 Rust workspace tests, host and Android release Clippy
under Rust 1.88, and seven package tests after the runtime-manifest update. It
changes neither the Java bootstrap nor dependencies and performs no publication.

The `.github/workflows/preview-package.yml` workflow runs on pushes that touch its
listed packaging/runtime/license paths, or by manual dispatch with a required
explicit preview version. Push builds use `<source-version>-preview.<GITHUB_RUN_NUMBER>`
as a CI test-package version, not a selected public release version. It uses a
macOS arm64 runner (`macos-15`), Rust 1.88, NDK 26.1.10909125, Android Build Tools
35.0.0, the API 29 platform JAR, Node 24 and JDK 17. It checks the plugin/scripts,
builds the archive, verifies `source.commit` equals the run's `GITHUB_SHA` and
`source.dirty` is false, and uploads `.tgz`/`.sha256` files as GitHub Actions
artifacts retained for 14 days. It has read-only repository contents permission
and performs no npm publication or GitHub Release. The downloaded artifact from
run `37652914514` passed provenance, native API 37, clean Web/Desktop CLI installation,
backend discovery and Desktop removal checks; GUI acceptance remains pending.
The source and staged package remain private; MPP uses Apache-2.0
with bundled third-party notices. See [RELEASING.md](RELEASING.md) for the remaining
acceptance and manual review steps.

## Compatibility baseline and prerequisites

Use the DSH source checkout at commit
`5badb15009ae1756c3afe0ae0cef1faafc290ccc`, with root/Desktop version
`0.2.1-alpha.1`. The launcher rejects another baseline. This is separate from any
installed Desktop release; no global installation or uninstall is needed.
Streaming/control work on this unpatched base. Optional automatic sidebar width
requires the local DSH UI patch below and a rebuild; that extension has not been
merged upstream or published as an official DSH API.

- Node.js 22.19+ on the 22.x line, or Node.js 24+; the same supported Node must be on
  `PATH` for DSH's subprocesses. Building also requires a complete Node installation
  with its matching headers for native addons. A runtime-only bundle without
  headers is insufficient for `--build`. A complete Node 24.4.0 installation built
  the tested source; the qualified development run uses Node 24.21.0 from DSH's
  locked runtime payload.
- Existing pnpm **11.7.0 exactly**, matching the source's `packageManager` pin.
  The launcher prefers the source's installed
  `apps/desktop/node_modules/pnpm` package, reading its `pnpm` bin entry (currently
  `bin/pnpm.mjs`) and invoking it with the selected Node.
  If absent, `pnpm` on `PATH` must report that exact version.
- DSH workspace dependencies installed using `pnpm install --frozen-lockfile` in
  that checkout, followed by `pnpm run build` for launches without `--build`.
  Desktop additionally needs its shell built (`pnpm run build:desktop`).
- An MPP executable built with `cargo build --workspace --locked` in this repository.
- Android SDK platform-tools for discovery; emulator tools and an existing AVD for
  explicit startup. Device discovery does not start an emulator.
- For live preview, the native library and bootstrap JAR described below, an
  authorized API 29–37 arm64 device, and client WebCodecs support for the actual
  H.264 profile emitted
  by its encoder. Unsupported browsers/codecs fail explicitly; there is no software
  decoder or screenshot-loop fallback. Other APIs, x86 emulators, Windows hosts and
  iOS remain outside the current live backend. The implementation range is not a
  compatibility claim across all devices or vendors; check the matrix.

The launcher installs no Node, pnpm, SDK or Rust components. DSH's stock build and
Desktop preparation scripts may download their own locked dependencies and prepare
the primary runtime. They are not an offline guarantee.

If Electron's binary has not been provisioned, DSH's Desktop README documents this
command from the source checkout:

```sh
pnpm --filter @deepseek-ai/dsh-desktop exec install-electron
```

Its Desktop development launcher also prepares the locked primary runtime before
opening the app. Allow that preparation to complete; an installed npm dependency
tree alone does not prove that the Electron or primary-runtime binaries exist.

## Automatic sidebar width: local DSH extension

The local `feat/sidebar-content-width` DSH branch adds temporary width requests:
`ctx.layout.requestRightbarWidth(width)` returns `update(width)` and `dispose()`,
and `tabInfo.panel.isSoleDockedPane` identifies a single docked pane. These are
local additions to the pinned source, not APIs already available upstream.

MPP derives screen height from available space after subtracting the phone shell's
vertical chrome, applies the encoded screen aspect ratio, and adds horizontal
chrome and section padding. The shell has a 4-pixel outer gutter per side, a
2-pixel rim and 8/10-pixel horizontal/vertical padding. Before geometry arrives
the screen aspect fallback is 9/20.
DSH retains its normal bounds: 300-pixel panel minimum, at most 70% of frame width,
and a 400-pixel center reservation. Thus the request is subject to available space.
User dragging cancels the current request, and MPP does not continually reassert
it. Leaving the active panel or entering fullscreen, split or floating layouts
releases the request and restores the stored width preference. An older/unpatched runtime uses
manual width; its media/control path is unchanged.

The earlier width-only patched Desktop passed review (95/100), before the phone
shell/automatic-resume increment. In a 1280×820 CSS viewport,
the right panel narrowed from 576 to 300 CSS pixels without reducing live-picture
height. A manual 430-pixel width remained after connection details expanded and
changed available preview height. Switching to the Start guide restored the saved
430-pixel preference; returning to MPP fitted again. Fullscreen stayed at full
viewport width, and window zoom/height changes adapted before the normal window
was restored. Split/float behavior has unit coverage, not separate live checks.
The local screenshot is `target/dsh/evidence/desktop-adaptive-sidebar.png`. Hidden
or occluded views suspend capture; wanted preview intent permits a fresh attempt
on return with a valid binding and without an explicit Pause. Auto-fit does
not promise capture keeps running in the background.

The patch is maintained in this repository at
`integrations/deepseek-harness/patches/0001-sidebar-content-width.patch`.
The artifact passed an application check against the clean pinned
commit and a reverse-application check against the modified local source tree.
Stop source development runtimes/watchers before applying and rebuilding it. Use
a dedicated DSH checkout whose `HEAD` is exactly
`5badb15009ae1756c3afe0ae0cef1faafc290ccc`; run these commands from the MPP root:

```sh
git -C /path/to/deepseek-harness rev-parse HEAD
git -C /path/to/deepseek-harness apply --check \
  "$PWD/integrations/deepseek-harness/patches/0001-sidebar-content-width.patch"
git -C /path/to/deepseek-harness apply \
  "$PWD/integrations/deepseek-harness/patches/0001-sidebar-content-width.patch"
```

Check the printed commit against the pin before applying. If `--check` fails,
inspect whether the patch is already applied or the checkout differs; do not force
it over another source version. The MPP launcher does not apply this patch itself.
Then choose one rebuild/launch command from the MPP root:

```sh
node scripts/dev-dsh.mjs web --source /path/to/deepseek-harness --build
node scripts/dev-dsh.mjs desktop --source /path/to/deepseek-harness --build
```

Reusing old built client artifacts is insufficient: both the local DSH layout
extension and MPP's client code must be loaded to exercise automatic sizing.

## Build the Android assets

Use an installed Android NDK, Rust `aarch64-linux-android` target, JDK 17 or later,
Android Build Tools with D8, and an Android platform JAR at API 29 or later:

```sh
./scripts/build-android-device.sh
```

The default `MPP_BUILD_PROFILE` is `debug`; set it to `release` for release
artifacts. The host/package build uses release; the standalone script accepts only
`debug` or `release`. The `codec_probe` path below uses `release` instead of `debug`
when that profile is selected. The bootstrap/library output directory is unchanged.

The script builds with Rust's locked dependency graph, compiles the small Java
entry with `--release 8`, and runs D8 with minimum API 29. It emits:

```text
target/android-device/bootstrap.jar
target/android-device/libmpp_android_device.so
target/aarch64-linux-android/debug/examples/codec_probe
```

Select installed toolchains with `ANDROID_NDK_HOME`, `JAVA_HOME` and, when needed,
`RUSTUP_TOOLCHAIN=1.88`. `MPP_BUILD_TOOLS` accepts an absolute build-tools version
directory; `MPP_ANDROID_JAR` accepts an absolute platform JAR. No SDK/Rust/JDK
installation is automatic. A newer compile platform does not widen the runtime
gate: both Rust ends use `android_framework(api)` for API 29–33 and 34–37 on arm64.
The Java bootstrap remains unchanged. The build checks every ELF LOAD segment for
at least 16 KiB alignment; all four segments in the inspected library passed. This
artifact check is separate from the successful API 37 native matrix run on one
emulator with an observed 16,384-byte page size.

`CARGO_TARGET_DIR` relocates these outputs. For a relocated build, provide the
absolute `android-device` directory through `MPP_DEVICE_ASSETS`. The launcher
otherwise configures the repository's `target/android-device` directory. Missing
assets leave discovery available but preview unavailable. `codec_probe` tests only
encoder initialization and intentionally reports `capture_verified: false`; use
the bootstrap/library pair for actual screen capture.

The opt-in native stream runner is documented in the
[Android compatibility matrix](ANDROID-COMPATIBILITY.md#reproduce-native-smoke-checks).
Use an exact authorized serial and put a non-launcher app such as Settings in the
foreground before an input-enabled run; the Home check needs an actual transition.
`--no-input` skips Home and cannot qualify input effects. Native matrix runs have
passed on API 30–37; stock DSH Web first-frame and input checks passed on API 30–34.
API 35–37 GUI remains unverified. The API 37 run also checked native operation on
16 KiB pages. The API 29 environment remains blocked by the independently
reproduced codec surface failure, including the final host-GPU/Vulkan-disabled
control. That failed capture left no packets but cleaned up its owned resources;
the matrix links the native and baseline evidence.

## Prepare and launch

From the MPP repository, first validate prerequisites and prepare configuration
without starting DSH:

```sh
node scripts/dev-dsh.mjs web --source /path/to/deepseek-harness --prepare-only
```

Launch one surface at a time for device testing:

```sh
node scripts/dev-dsh.mjs web --source /path/to/deepseek-harness --port 3081
node scripts/dev-dsh.mjs desktop --source /path/to/deepseek-harness
```

Add `--build` to build before launching. Web otherwise invokes the stock
`pnpm run dev:web --skip-build --no-open --port 3081` workflow; Desktop uses
`pnpm run start:desktop`, or `pnpm run dev:desktop` with `--build`.
Web also receives the source's `apps/web/tests/pin-browse-picker.overlay.yml`,
which makes the folder picker available in the browser for UI testing.
Do not run a separate DSH build while its Web development watchers are active.
This flag builds DSH, not MPP or its Android assets; build those separately first.

Optional environment variables:

| Variable | Purpose |
| --- | --- |
| `MPP_EXECUTABLE` | Absolute MPP executable; default `target/debug/mpp` in this repository |
| `MPP_ADB` | Absolute adb executable override |
| `MPP_EMULATOR` | Absolute Android emulator executable override |
| `MPP_DEVICE_ASSETS` | Absolute directory containing the bootstrap JAR and native library; default `target/android-device` |
| `MPP_PNPM` | Absolute existing pnpm 11.7.0 executable override; otherwise prefers the source-installed copy, then checks `PATH` |
| `DSH_DESKTOP_MAIN_INSPECT_PORT` | Desktop Main inspector; default `9239` |
| `DSH_DESKTOP_RENDERER_DEBUG_PORT` | Desktop renderer debugger; default `9232` |
| `DSH_DESKTOP_HOST_INSPECT_PORT` | Desktop Host inspector; default `9240` |
| `DSH_DESKTOP_OPEN_DEVTOOLS` | Defaults to `0`; set `1` to open Desktop DevTools |

MPP path overrides are validated and written as absolute YAML string literals;
the generated patch does not depend on DSH's environment-expression evaluation.
If `MPP_ADB` is set, also set `MPP_EMULATOR` when AVD discovery/startup is needed.

The plugin's public configuration also accepts the following stream settings;
these are host configuration, never browser-supplied file paths or codec arguments:

| Setting | Default | Allowed values |
| --- | --- | --- |
| `deviceAssets` | Bundled Android assets for a packed install; launcher-provided for source development | Absolute asset directory |
| `videoMaxSize` | `1280` | Even integer, 256–2048; encoded dimensions are aligned by the backend |
| `videoBitRate` | `4000000` | 100,000–20,000,000 bits/s |
| `videoMaxFps` | `30` | 1–60 fps |
| `previewTimeoutMs` | `45000` | 30,000–120,000 ms service/browser request budget |

The internal start/stop RPC limits remain 45/30 seconds; increasing the outer budget
does not extend them. Requested fps/bitrate are settings, not performance guarantees.
The development launcher writes paths but exposes no CLI flags for video tuning.

## Use the preview

1. Open a DSH conversation and its mobile-device panel. Refresh inventory and choose
   the exact authorized device in the implemented API/ABI range; explicitly start an AVD if needed.
2. Choose **Connect**. A new binding enables preview automatically once its canvas
   is visible. `transport_ready` is only the connection state: video still requires
   backend/codec checks and a decoded first frame, with a ten-second frame deadline.
3. Click or drag inside the displayed picture. The panel provides Home/Back buttons;
   a focused canvas also maps arrows, Enter, Backspace, Tab and Space. It does not
   capture keyboard events globally. Multi-touch, text/IME, clipboard, audio and
   recording are not implemented.
4. **Pause** keeps preview stopped across resizing, hiding and returning; choose
   **Resume** to enable it again. Ordinary hiding/backgrounding instead suspends
   capture and releases input while retaining preview intent. Returning to a visible
   canvas resumes automatically after the previous capture finishes cleanup, as
   long as the binding is valid. Closing an old view cannot revive that view.
5. Genuine failures latch within the current visible view. **Retry**/**Resume** or
   a new visible view/visibility return may attempt once again while intent and
   binding remain valid. Routine rerenders and heartbeats never clear the latch;
   there is no timer/hot retry loop. Device rotation or display-geometry changes
   terminate their capture epoch, without replaying input. **Disconnect**, expiry
   and plugin unload clear intent; a stale device connection is never automatically
   reconnected.

The phone body is presentation only: a rounded metal-style rim, earpiece and side
buttons surround the actual screen. Decorations are noninteractive and outside
the canvas; screen aspect ratio and input mapping remain tied to device geometry.
Shell-aware width fitting reuses the existing local DSH patch. This increment
changes MPP, not DSH. Its Desktop runtime checks reached Live from a new connection
using Connect alone; tapping Apps inside the framed Settings screen opened Apps,
and Home worked. Reported window width 2560→2700 remained Live with one helper PID;
OS window zoom 2700→3024 and back also kept its helper PID within that separate check.

Collapsing/reopening the panel resumed Starting→Live after old-helper cleanup,
without BUSY. Explicit Pause left zero helpers and remained paused across panel
collapse/reopen; Resume returned to Live. The full plugin check passed 133 tests,
including 13 lifecycle and 4 frame tests. Background document events are covered
by unit tests, not a separate real OS minimize/background-return test. The local
screenshot is `target/dsh/evidence/desktop-auto-preview-device-frame.png`. Visual
review passed (96/100): rim, earpiece and side buttons remain outside the screen,
with no stretching or obscured status-bar/app content; controls are readable and
the sidebar remains compact. The old unchanged-picture-height
measurement preceded the phone frame; its chrome now intentionally uses some height.

Input replies acknowledge OS submission, not application success. Input is never
automatically replayed after an uncertain failure. Losing focus/capture, closing a
stream or losing controller traffic releases pressed input; the device watchdog
is separate from the browser-client heartbeat. These are implementation behaviors
to verify on real devices before broad reliability claims.

## State and plugin loading

The launcher uses these directories under this repository:

```text
target/dsh/web-home/
target/dsh/desktop-home/
target/dsh/desktop-user-data/
```

Homes and Desktop user data are separate from the normal DSH application. The
launcher does not copy credentials, settings or sessions from existing profiles.
No DSH source changes are needed for streaming/control or manual panel sizing;
automatic width uses the optional local UI patch above. The launcher creates its state directories
with mode `0700` and patches with mode
`0600`. Each home receives `cordis.patch.yml`, containing an absolute loader row
for `packages/dsh-plugin/index.js`. A generated-file marker permits idempotent
updates; an existing unrecognized patch is preserved and reported as an error.

DSH's existing client module discovery finds the package's `dsh.client` metadata
and `./client` export from the absolute host entry. No DSH core registration,
global package install or official merge is required. The source CLI intentionally
cannot manage the reserved Desktop profile; this setup uses the supported home
patch layer instead. The Desktop Host chooses a free listening port automatically.

Ctrl+C or SIGTERM is forwarded to the owned launcher child and its exit is awaited.
MPP never searches for and kills another Desktop, adb server or emulator. Review
the stock DSH process output if it reports a shutdown failure.

## Checks and limits

```sh
node --check scripts/dev-dsh.mjs
npm --prefix packages/dsh-plugin run check
node --test scripts/tests/*.test.mjs
```

The initial streaming qualification used a fully built, unmodified pinned DSH
source, before the later local sidebar-width extension. Web at
`http://127.0.0.1:3081` and the native Harness Dev Desktop launched with independent
homes, loaded the shared MPP composer button/panel, and listed four local AVDs.
The Web panel booted `Pixel_6_API_32`, reached `transport_ready` and disconnected.
Native Desktop also reached `transport_ready`. The newer streaming path has decoded
and rendered actual emulator H.264 in Web. Canvas tap opened Settings → Apps, Back
returned to Settings, dragging scrolled, and Home reached the launcher. Local visual
evidence is `target/dsh/evidence/web-live-control.jpg`. Native Home down/up also moved
the resumed activity from Settings to NexusLauncher.

The native path ran for 600.1 seconds, initially static for 180 seconds and then
receiving Home-key pairs every 30 seconds. It stayed healthy and reported 1
configuration packet, 8 keyframes, 228 delta frames and 1,184,670 bytes. Touch was
released at the 2.6-second watchdog check and after control EOF. Native 90°/180°
rotation checks ended streams as expected; 10/10 fresh start/stop cycles cleaned up
and device settings were restored. This is not a sustained Web/Desktop playback,
throughput or latency benchmark.

Desktop produced its first decoded frame after normal decoder-backlog handling
was corrected. Home moved Calendar → launcher; dragging opened the app drawer;
taps opened Settings → Apps; Back returned to Settings. The preview remained live
through sidebar resizing during about two minutes of interaction. A checked stop
left zero MPP native helpers and owned adb reverse mappings. A subsequent clean
Desktop process restart reproduced a decoded live frame with temporary diagnostics
removed and DSH source still unchanged at that qualification point. The final
live-frame screenshot is
`target/dsh/evidence/desktop-live-control.png`. A competing preview had also been
refused without interrupting an active capture.

Normal `decodeQueueSize >= 4` now waits for `dequeue` for at most two seconds while
preserving ordered reads, rather than resetting the decoder and unnecessarily
waiting for a new IDR on a static display. Actual decoder-error recovery remains
bounded reset/IDR recovery; it may need a new producer frame or preview restart.
An earlier unexplained Web EOF did not recur in the native run or latest Web
checks; this separate symptom has no proven root cause or claimed fix.

Tests cover native protocol/input state, MPP1 parsing, decoder recovery, control
sequence/epoch checks, bounded video backpressure and lifecycle cancellation. The
packaging tests use fake installed tools and require no SDK or device. Passing
these checks is not a full compatibility matrix. Further qualification must cover
additional GUI restart/reconnect cycles, focus loss, unload, backend failure and
sustained GUI playback in both pinned clients, plus additional devices and versions.

Connection ownership is local to one Rust Host process. The native helper additionally
rejects competing MPP capture processes through a device-side abstract-socket lock;
this does not prevent external adb commands or physical touches. Run Web/Desktop
device checks sequentially. Web displays devices attached to the DSH Host
machine, not automatically to the browser's machine. No continuous frames enter
the model context, and a panel does not itself grant agent vision/control.

MPP remains a private development preview: no public package release or official
DSH inclusion is claimed, and no release version or date is promised here.
