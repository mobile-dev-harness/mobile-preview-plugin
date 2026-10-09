# 0.1.0-preview.5 release record

The repository is public and licensed under [Apache-2.0](../LICENSE).
[**`v0.1.0-preview.5` is the first published GitHub prerelease**](https://github.com/mobile-dev-harness/mobile-preview-plugin/releases/tag/v0.1.0-preview.5),
covering existing Android support plus local iOS Simulator. It is not a draft.
Publication completed at **2026-10-09T18:09:34Z** (**2026-10-10** in Asia/Shanghai).
CI, provenance and the exact-archive checks below passed, including Web GUI
removal/reinstallation and the final native App Switcher check.
The npm package remains `private: true`, and Rust crates retain `publish = false`.
This is an unofficial development preview, not a 1.0 stable release or an official
DeepSeek distribution component.

## Published artifact identity and acceptance

These fields identify the immutable published artifact. This later
documentation update reports its evidence without rebuilding it or changing its
source commit. The release tag remains on that artifact's source commit.
Historical source-build and archive checks do not transfer to another checksum.

| Item | Current status |
| --- | --- |
| GitHub Release | [`v0.1.0-preview.5`](https://github.com/mobile-dev-harness/mobile-preview-plugin/releases/tag/v0.1.0-preview.5), published prerelease, not a draft |
| Package | [macOS Apple Silicon `.tgz`](https://github.com/mobile-dev-harness/mobile-preview-plugin/releases/download/v0.1.0-preview.5/mobile-dev-harness-dsh-mobile-preview-0.1.0-preview.5-darwin-arm64.tgz) and [`.sha256`](https://github.com/mobile-dev-harness/mobile-preview-plugin/releases/download/v0.1.0-preview.5/mobile-dev-harness-dsh-mobile-preview-0.1.0-preview.5-darwin-arm64.tgz.sha256) |
| Artifact source commit and release-tag target | `31a2bdb1c4d96345c7735377d6d861fe52ce12a4` |
| Manifest source-tree SHA-256 | `6d2a54a247cdb0015052550a6c92b1b8590a090f6d2f835fe9c468816c29e9c8` |
| CI | [Run 37965376388](https://github.com/mobile-dev-harness/mobile-preview-plugin/actions/runs/37965376388): all six jobs passed |
| Packaging | [Run 37965378783](https://github.com/mobile-dev-harness/mobile-preview-plugin/actions/runs/37965378783): passed |
| Archive SHA-256 | `04cd7427bdb45bb00d823d789a2573969711a93cee1da837132520703f59f2af` |
| Bundled native host SHA-256 | `aab0a241a91b5903bba932cf3114172739fe441ca1c69e7025da971892c0dcc9` |
| Qualification asset | [qualification-0.1.0-preview.5.json](https://github.com/mobile-dev-harness/mobile-preview-plugin/releases/download/v0.1.0-preview.5/qualification-0.1.0-preview.5.json) |
| Qualification asset SHA-256 | `8dbe1abc93afddbb10d067ddc9b47781c870be3bfaf3ea1a0522465f641fd578` |
| Provenance, native hashes and licenses | Verified: clean manifest source, matching artifact commit/tree, all bundled native-file hashes and license inventory/notices |
| Final CI tests and toolchains | Passed: 153 macOS Rust tests (one device probe ignored), 185 plugin tests, 46 script tests, formatting, strict Clippy and MSRV Rust 1.88; all six CI jobs passed |
| Exact-archive runtime acceptance | Passed for the scoped Web, Desktop, native cleanup/reboot, App Switcher and Android checks below; limitations stay separate |
| GitHub Release publication | Completed at `2026-10-09T18:09:34Z`; prerelease, not draft |
| npm publication | Outside this release; package remains private |

## Release scope

| Area | Included scope and evidence boundary |
| --- | --- |
| Host/client | macOS Apple Silicon Host; WebCodecs-capable DSH Web/Desktop clients. The host package targets macOS 14+, while current iOS live evidence uses macOS 26.7. |
| Android | Existing API 29–37 arm64-v8a adapters, exact device selection, explicit AVD startup, H.264 preview, single-pointer input and basic keys. Adapter eligibility is not a full runtime matrix; API 29 remains blocked in the tested environment. |
| iOS | Local Simulator only: Xcode 26.4 (17E192), iOS 26.4, iPhone 17 on the recorded Apple Silicon host. Other Xcode/runtime/Simulator combinations are unqualified. |
| iOS controls | Capability-gated single-pointer tap/drag, Home and bottom-edge Home swipe; missing native input capability keeps read-only video. Exact-archive checks passed in stock Web 0.2.1-alpha.1 and official Desktop 0.2.0-rc.2. |
| App Switcher | Exact-archive timed native gesture passed, with the result observed through freshly reinstalled Web. Earlier user-operated Web evidence is dated 2026-10-10. Manual Desktop App Switcher remains unqualified. |
| Agent tool | Optional `open_mobile_preview` selects the current chat's platform panel. It does not boot/connect devices, send input, capture screenshots or grant model vision. |

The iOS preview is upright portrait with fixed capture geometry. Orientation,
geometry or boot changes terminate the capture. It uses private CoreSimulator
interfaces checked at runtime; installed Xcode and Apple frameworks are not
redistributed. DTUHID cleanup ends held gestures and may complete the tap or drag;
it does not guarantee UIKit cancellation without activation.

Physical iOS discovery, connection, input and preview are excluded. iOS
keyboard/text/IME, multi-touch, Back, audio and recording are not included.
Remaining live gaps include rotation/geometry rejection and manual Desktop App
Switcher. Android physical-device rotation, hot unplug, broad vendor coverage
and sustained sessions also retain their existing limits.

## Exact-archive runtime checks

The downloaded archive identified above was installed in fresh profiles on the
recorded macOS 26.7 Apple Silicon / Xcode 26.4 / iOS 26.4 / iPhone 17 target.

| Check | Observed result |
| --- | --- |
| Stock Web 0.2.1-alpha.1 startup | Passed: plugin UI explicitly booted the selected stopped Simulator; Connect automatically reached the first decoded frame |
| Web input and resizing | Passed: Settings tap, visible drag scrolling, Home button, bottom Home swipe, and accurate tap after resizing the viewport to 800×900 |
| Web preview lifecycle | Passed: Pause, paused panel reopening and Resume; paused capture child was reaped |
| Official Desktop 0.2.0-rc.2 | Passed: fresh installation, first frame, tap/drag/Home/bottom Home swipe, Pause/Resume and disconnect |
| Native cleanup | Passed: STOP, stdin EOF and SIGTERM each exited with code 0, without forced termination, and removed owned sockets |
| Actual Simulator reboot | Passed: the old lease returned `STALE_SESSION`; reconnection used a new boot identity and new generation |
| Android API 35 arm64 | Passed: five-second native stream/cleanup smoke without input; this does not renew Android input qualification |
| Web GUI removal/reinstallation | Passed: Plugin Manager first hid the connected preview; GUI removal cleared dependency/bundle/package link and owned host/capture. GUI Add of the local `.tgz` plus Enable Now restored identical native hashes, and Connect returned to Live |
| Final native App Switcher | Passed: timed bottom swipe/hold through the CI-built host produced actual App Switcher cards observed through freshly reinstalled Web |

The Desktop check used `--force-renderer-accessibility` solely to expose the
renderer accessibility tree to test automation. It changed neither media nor application code.
Source-build input and historical archive records below remain separate evidence.
The final [reinstalled-Web App Switcher screenshot](../target/release-simulator/evidence/web-reinstalled-app-switcher.png)
is local evidence; the published qualification JSON provides the durable release record.

## Build and verify the exact artifact

Use the [packaging guide](DSH-DEVELOPMENT.md#build-a-self-contained-preview-package)
with the toolchains recorded in the license inventory, including Rust 1.88.0 and
Android NDK 26.1.10909125:

```sh
RUSTUP_TOOLCHAIN=1.88 node scripts/package-preview.mjs --version 0.1.0-preview.5
```

The script rejects output collisions, toolchain/license-inventory drift and source
changes during packaging. It includes the host, Android assets, plugin runtime,
license and third-party notices. iOS uses the host executable and installed Xcode;
it needs no Android JAR or `.so` at runtime. Packaging or a workflow artifact upload
alone does not publish a GitHub Release.

- [x] Commit existing Android plus Simulator-only iOS scope; ensure unfinished physical-iOS implementation
      and private local evidence are absent from the published source and package.
- [x] Run formatting, workspace tests, Clippy with warnings denied, Rust 1.88/MSRV,
      plugin checks, script tests and the Android-target checks. Record exact results.
- [x] Build or retrieve the `0.1.0-preview.5` CI artifact and its checksum. Record the
      workflow URL, full commit SHA, artifact name and archive SHA-256 above.
- [x] Verify `runtime-manifest.json`: matching commit, `source.dirty: false`,
      `source.treeSha256`, versions, targets and every bundled native-file hash.
- [x] Confirm `LICENSE`, `THIRD-PARTY-NOTICES.md` and `licenses/third-party/` match
      the bundled dependencies. Keep Xcode, Apple frameworks and idb runtime out of
      the redistributed archive.
- [x] Install that exact archive in clean DSH profiles and retain scoped Android
      and iOS runtime evidence tied to its checksum. Cover video and supported
      controls; keep historical checks distinct from new artifact checks.
- [x] Finish the exact-archive Web GUI removal/reinstallation and native App Switcher checks.
- [x] Check README language parity and release notes against the scope and gaps
      above; do not infer new device or lifecycle passes from older source builds.
- [x] Publish the reviewed archive, checksum and qualification JSON as GitHub prerelease
      `v0.1.0-preview.5`, then record the actual release URL and publication result.
      Keep npm private and retain provenance/evidence beyond CI artifact expiry.

The `Build preview package` workflow produces artifacts, not releases. Its
run-number versions are CI test identities; use the explicit release version
for this release. Checksums and runtime manifests establish integrity/provenance,
not signatures. DSH Desktop notarization does not sign the MPP archive.

## Historical evidence retained for scope

These records explain what has already been observed. They are not final
`0.1.0-preview.5` artifact acceptance.

- [Android compatibility matrix](ANDROID-COMPATIBILITY.md): the local preview.3
  matrix passed native capture/input/cleanup on API 30–37 and stock Web on API
  30–34, including native API 37 with 16 KiB pages. API 29 codec failure remains.
- The separately downloaded preview.3 CI archive from
  [run 37652914514](https://github.com/mobile-dev-harness/mobile-preview-plugin/actions/runs/37652914514),
  commit `9ec7afad96f486b8050f2f41e6ca8bade24c9b8c`, had SHA-256
  `d6f6450826638bdb44cce5a2b8669965036f7de87ec8d79e2468d3b5d08f9b9c`.
  It passed native API 37, stock Web API 35–37, and official Desktop API 37
  first-frame/input checks plus the recorded Pause/Resume, resize/zoom, removal
  and reinstallation cases. The same version number does not merge its evidence
  with the local matrix archive. Details are in
  [DSH development](DSH-DEVELOPMENT.md), with local records under
  `target/release-readiness/{native,remote-artifact,native-ci,desktop-ci,web-ci,gui}`.
  An earlier cold-boot native attempt did not observe Home taking effect; retries
  passed but the cause was not fully isolated. Its original record remains in
  `target/release-readiness/native/summary.json`.
- Earlier preview.1 Web installation and preview.2 API 36 nubia physical-phone
  results remain archive-specific history, not passes for the new release or
  other physical devices. See the Android matrix and development guide.
- [iOS Simulator qualification](IOS-SIMULATOR.md): separate read-only video,
  input-feature and bottom-edge increments retain their original binary hashes,
  test counts and live evidence. Source Web/Desktop Home/tap/drag and bottom Home
  swipes passed; user-operated Web App Switcher passed on 2026-10-10. Desktop
  manual App Switcher and the listed lifecycle gaps remain unqualified.
- Local iOS evidence is under `target/ios-preview/`, `target/ios-input/`,
  `target/ios-edge/` and `target/ios-home-gesture/`. Those ignored files are local
  evidence, not automatically included in a public source checkout or release.
