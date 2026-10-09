# 0.1.0-preview.5 release checklist

The repository is public and licensed under [Apache-2.0](../LICENSE).
**`0.1.0-preview.5` is the first public GitHub Release candidate**, covering
existing Android support plus local iOS Simulator. GitHub publication is
authorized; final CI artifact acceptance and publication are still pending.
The npm package remains `private: true`, and Rust crates retain `publish = false`.
This is an unofficial development preview, not a 1.0 stable release or an official
DeepSeek distribution component.

## Candidate identity and acceptance

Fill these fields from the exact artifact selected for publication. Historical
source-build and archive checks below do not transfer to a new checksum.

| Item | Current status |
| --- | --- |
| GitHub Release version | `0.1.0-preview.5` |
| Package | `mobile-dev-harness-dsh-mobile-preview-0.1.0-preview.5-darwin-arm64.tgz` plus adjacent `.sha256` |
| Final source commit | Pending final commit |
| CI workflow run and artifact | Pending |
| Archive SHA-256 | Pending |
| Runtime manifest and bundled native hashes | Pending verification against the chosen archive |
| Local format, lint, tests and MSRV | Passed: 153 Rust tests (one device probe ignored by default), 185 plugin tests, formatting, strict Clippy and Rust 1.88; 14 packaging fixture tests passed after the packaging metadata update. Final CI results pending. |
| Exact-archive installation and scoped runtime checks | Pending |
| GitHub Release publication | Authorized; not yet confirmed |
| npm publication | Outside this release; package remains private |

## Release scope

| Area | Included scope and evidence boundary |
| --- | --- |
| Host/client | macOS Apple Silicon Host; WebCodecs-capable DSH Web/Desktop clients. The host package targets macOS 14+, while current iOS live evidence uses macOS 26.7. |
| Android | Existing API 29–37 arm64-v8a adapters, exact device selection, explicit AVD startup, H.264 preview, single-pointer input and basic keys. Adapter eligibility is not a full runtime matrix; API 29 remains blocked in the tested environment. |
| iOS | Local Simulator only: Xcode 26.4 (17E192), iOS 26.4, iPhone 17 on the recorded Apple Silicon host. Other Xcode/runtime/Simulator combinations are unqualified. |
| iOS controls | Capability-gated single-pointer tap/drag, Home and bottom-edge Home swipe; missing native input capability keeps read-only video. Source checks passed in stock Web 0.2.1-alpha.1 and official Desktop 0.2.0-rc.2. |
| App Switcher | Timed native protocol gesture and actual user-operated Web gesture passed; the manual Web check was on 2026-10-10. Manual Desktop App Switcher remains unqualified. |
| Agent tool | Optional `open_mobile_preview` selects the current chat's platform panel. It does not boot/connect devices, send input, capture screenshots or grant model vision. |

The iOS preview is upright portrait with fixed capture geometry. Orientation,
geometry or boot changes terminate the capture. It uses private CoreSimulator
interfaces checked at runtime; installed Xcode and Apple frameworks are not
redistributed. DTUHID cleanup ends held gestures and may complete the tap or drag;
it does not guarantee UIKit cancellation without activation.

Physical iOS discovery, connection, input and preview are excluded. iOS
keyboard/text/IME, multi-touch, Back, audio and recording are not included.
Remaining live gaps include explicit Simulator boot through the plugin UI,
rotation/geometry rejection, actual reboot invalidation and full plugin uninstall
with iOS capture. Preserve these limitations in release notes rather than marking
them passed. Android physical-device rotation, hot unplug, broad vendor coverage
and sustained sessions also retain their existing limits.

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

- [ ] Commit existing Android plus Simulator-only iOS scope; ensure unfinished physical-iOS implementation
      and private local evidence are absent from the published source and package.
- [ ] Run formatting, workspace tests, Clippy with warnings denied, Rust 1.88/MSRV,
      plugin checks, script tests and the Android-target checks. Record exact results.
- [ ] Build or retrieve the `0.1.0-preview.5` CI artifact and its checksum. Record the
      workflow URL, full commit SHA, artifact name and archive SHA-256 above.
- [ ] Verify `runtime-manifest.json`: matching commit, `source.dirty: false`,
      `source.treeSha256`, versions, targets and every bundled native-file hash.
- [ ] Confirm `LICENSE`, `THIRD-PARTY-NOTICES.md` and `licenses/third-party/` match
      the bundled dependencies. Keep Xcode, Apple frameworks and idb runtime out of
      the redistributed archive.
- [ ] Install that exact archive in clean DSH profiles and retain scoped Android
      and iOS runtime evidence tied to its checksum. Cover video and supported
      controls; keep historical checks distinct from new artifact checks.
- [ ] Check README language parity and release notes against the scope and gaps
      above; do not infer new device or lifecycle passes from older source builds.
- [ ] Publish the reviewed archive and checksum as GitHub Release
      `0.1.0-preview.5`, then record the actual release URL and publication result.
      Keep npm private and retain provenance/evidence beyond CI artifact expiry.

The `Build preview package` workflow produces artifacts, not releases. Its
run-number versions are CI test identities; use the explicit candidate version
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
  results remain archive-specific history, not passes for the new candidate or
  other physical devices. See the Android matrix and development guide.
- [iOS Simulator qualification](IOS-SIMULATOR.md): separate read-only video,
  input-feature and bottom-edge increments retain their original binary hashes,
  test counts and live evidence. Source Web/Desktop Home/tap/drag and bottom Home
  swipes passed; user-operated Web App Switcher passed on 2026-10-10. Desktop
  manual App Switcher and the listed lifecycle gaps remain unqualified.
- Local iOS evidence is under `target/ios-preview/`, `target/ios-input/`,
  `target/ios-edge/` and `target/ios-home-gesture/`. Those ignored files are local
  evidence, not automatically included in a public source checkout or release.
