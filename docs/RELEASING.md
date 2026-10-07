# Preview release readiness

MPP is an unofficial development preview licensed under [Apache-2.0](../LICENSE).
The source crates retain `publish = false` and the plugin retains `private: true`.
No npm publication or GitHub Release has been authorized or performed. Local
packaging and GitHub Actions artifact uploads do not publish a release.

## Evidence and remaining acceptance

The [Android matrix](ANDROID-COMPATIBILITY.md) records `0.1.0-preview.3`, with
archive identity and per-target evidence in
`target/android-matrix/verification.json`. Native capture/input/cleanup passed on
the API 30–37 targets, including one API 37 emulator with 16 KiB pages. Stock DSH
Web GUI first-frame/input checks passed on API 30–34. The official Desktop
`0.2.0-rc.2` app passed signature/notarization checks and installed preview.3 through
its bundled CLI in an isolated profile with the bundle enabled and native hashes
matching. These results apply to those tested archives, targets and surfaces.

Additional native checks on the preview.3 components covered a 60-second stream,
three reconnects, control EOF, stdin EOF, SIGTERM and emulator transport loss with
recovery in the same host process. Final runs passed and left no owned helper,
reverse mapping or stream directory. The first cold-boot recovery attempt did not
observe Home taking effect; the independent retry and full retry after checking
Android input readiness passed. Its cause is not fully isolated, so retain that
attempt alongside the successful report at
`target/release-readiness/native/summary.json`. This is neither a physical USB
unplug test nor a long GUI-session qualification.

The [remote packaging run](https://github.com/mobile-dev-harness/mobile-preview-plugin/actions/runs/37652914514)
and [six-job CI run](https://github.com/mobile-dev-harness/mobile-preview-plugin/actions/runs/37652914657)
passed for commit `9ec7afad96f486b8050f2f41e6ca8bade24c9b8c`. Its downloaded
`0.1.0-preview.3` CI archive has SHA-256
`d6f6450826638bdb44cce5a2b8669965036f7de87ec8d79e2468d3b5d08f9b9c`.
This CI run-number version is distinct from the earlier local preview.3 matrix
archive; identify them by checksum and source provenance, not version alone.

The exact downloaded archive passed native capture/Home and SIGTERM cleanup on
API 37 with 16 KiB pages. It installed into fresh stock Web `0.2.1-alpha.1` and
official Desktop `0.2.0-rc.2` profiles; both enabled the plugin and discovered the
device through its authenticated backend. Desktop removal cleared the dependency,
bundle entry, package link and native host; after restart its plugin route returned
404. Reinstallation restored the backend. An earlier local preview.3-to-preview.4
CLI upgrade also passed. These are installation/backend checks, not visible
first-frame or touch verification. Evidence is under
`target/release-readiness/{remote-artifact,native-ci,desktop-ci,web-ci}`.

Before selecting a first public preview, complete or explicitly narrow the stated
qualification scope for these open checks:

- Stock DSH Web GUI on API 35–37: decoded first frame and visible tap/drag/navigation
  effects. Browser control became unavailable after the API 34 check.
- Official Desktop GUI: clean installation, first frame, tap/drag/Home/Back,
  Pause/Resume across panel reopening, and removal while connected with host,
  helper and owned reverse mappings cleaned up. The CLI installation evidence at
  `target/release-readiness/desktop/evidence/install-preview3.json` does not cover
  these GUI checks.
- API 29 native and GUI checks on a working target. The tested emulator codec
  surface failed independently in MPP, scrcpy and system screenrecord; cleanup
  passed, but capture/input remains unverified.
- Record the limits of physical-device coverage, rotation, hot unplug and sustained
  GUI sessions. The earlier API 36 nubia phone and source Desktop passes are
  historical evidence, not passes for every later archive or vendor.
- Complete GUI acceptance of the exact selected remote archive before publication.

## Build and identify the candidate

Use the [packaging guide](DSH-DEVELOPMENT.md#build-a-self-contained-preview-package)
with installed Rust 1.88.0 and Android NDK 26.1.10909125, matching the license
inventory, and an explicit `<source-version>-preview.<number>` version:

```sh
RUSTUP_TOOLCHAIN=1.88 node scripts/package-preview.mjs --version 0.1.0-preview.4
```

`0.1.0-preview.4` is a development example, not the selected public version.
The script refuses existing output files, license-inventory drift and source
changes during the build. It includes `LICENSE`, `THIRD-PARTY-NOTICES.md` and
`licenses/third-party/` and emits an adjacent archive checksum.

Before acceptance, run Rust formatting, workspace tests and Clippy with warnings
denied, plugin checks, script tests and Android-target Clippy. Retain the command
results with the candidate evidence. Use locked Cargo dependencies and the same
toolchain/build settings as the archive; tests without a device do not replace
native or GUI qualification.

## Match a remote artifact to its commit

The `Build preview package` workflow runs on relevant path pushes and manual
dispatch. Push versions are `<source-version>-preview.<GITHUB_RUN_NUMBER>` CI test
versions; manual dispatch requires an explicit version. Neither trigger selects
or publishes the public version. Artifacts are retained for 14 days.

For the accepted workflow run, record its URL, full commit SHA, package version,
artifact name and archive SHA-256. Download its `.tgz` and `.sha256`, verify the
archive checksum, and inspect `package/runtime-manifest.json` inside the archive:

- `source.commit` must equal the accepted workflow run's full commit SHA, and
  `source.dirty` must be `false`. The workflow enforces both before upload.
- Record `source.treeSha256`, the fingerprint of the source contents used for the
  build. A local archive with `dirty: true` is not identified by its commit alone.
- Check package/host versions, targets and each bundled native-file SHA-256
  against the manifest. Confirm all license and notice files are present.
- Install that exact archive in clean test profiles and attach its checksum to
  the acceptance evidence. Do not substitute a locally rebuilt archive or infer
  GUI success from an older package's tests.

Checksums and the runtime manifest are integrity and provenance records, not
signatures. The official Desktop app's signature/notarization status does not
sign or notarize the MPP archive.

## Manual review before publication

Review the exact archive, source commit, checksums, test evidence, license inventory
and release notes together. Ensure both READMEs describe the same qualification
scope and list unresolved limitations without turning unverified targets into
passes. Choose the public version and destination, and obtain explicit publication
authorization for that concrete candidate. Keep `private: true` and the current
artifact-only workflow until a separately reviewed publishing change is needed.

After publication is authorized, distribute only the reviewed artifact and retain
its provenance and evidence beyond the workflow's artifact-retention window.
