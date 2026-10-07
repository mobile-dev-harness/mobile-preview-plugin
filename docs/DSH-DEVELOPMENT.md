# DSH source development

The MPP adapter is a local external plugin for DSH Web and Desktop. Its JavaScript
host/UI layer delegates Android discovery and session operations to the Rust MPP
process. This stage provides a connection panel; it does not provide video preview,
input injection or agent screenshot tools. No mdh installation is required.
The adapter has no third-party Node dependencies and uses DSH's runtime-provided
React/UI services. Its authenticated DSH route forwards to an internal Rust stdio
protocol; it does not register MCP or agent tools.

## Compatibility baseline and prerequisites

Use the DSH source checkout at commit
`5badb15009ae1756c3afe0ae0cef1faafc290ccc`, with root/Desktop version
`0.2.1-alpha.1`. The launcher rejects another baseline. This is separate from any
installed Desktop release; no global installation or uninstall is needed.

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

Optional environment variables:

| Variable | Purpose |
| --- | --- |
| `MPP_EXECUTABLE` | Absolute MPP executable; default `target/debug/mpp` in this repository |
| `MPP_ADB` | Absolute adb executable override |
| `MPP_EMULATOR` | Absolute Android emulator executable override |
| `MPP_PNPM` | Absolute existing pnpm 11.7.0 executable override; otherwise prefers the source-installed copy, then checks `PATH` |
| `DSH_DESKTOP_MAIN_INSPECT_PORT` | Desktop Main inspector; default `9239` |
| `DSH_DESKTOP_RENDERER_DEBUG_PORT` | Desktop renderer debugger; default `9232` |
| `DSH_DESKTOP_HOST_INSPECT_PORT` | Desktop Host inspector; default `9240` |
| `DSH_DESKTOP_OPEN_DEVTOOLS` | Defaults to `0`; set `1` to open Desktop DevTools |

MPP path overrides are validated and written as absolute YAML string literals;
the generated patch does not depend on DSH's environment-expression evaluation.
If `MPP_ADB` is set, also set `MPP_EMULATOR` when AVD discovery/startup is needed.

## State and plugin loading

The launcher uses these directories under this repository:

```text
target/dsh/web-home/
target/dsh/desktop-home/
target/dsh/desktop-user-data/
```

Homes and Desktop user data are separate from the normal DSH application. The
launcher does not copy credentials, settings or sessions from existing profiles.
No changes to DSH core source are needed. The
launcher creates its state directories with mode `0700` and patches with mode
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
```

The pinned DSH source completed a full build and remained clean. Web at
`http://127.0.0.1:3081` and the native Harness Dev Desktop launched with independent
homes, loaded the shared MPP composer button/panel, and listed four local AVDs.
The Web panel booted `Pixel_6_API_32`, reached `transport_ready` and disconnected.
Native Desktop also reached `transport_ready`. No video or input was exercised.

The adapter checks pass 56 tests: 19 bridge, 10 client, 6 entry and 21 service tests.
The existing 52 Rust tests also pass; this integration stage did not change Rust
code. These counts describe the current baseline, not a full device compatibility
matrix. Further lifecycle checks should cover panel/chat transitions, unload,
missing tools, unauthorized devices and disconnect/reconnect behavior before wider
claims. The UI states whether closing the panel retains the chat's lease; do not
infer that behavior from the presence of a sidebar alone.

Device ownership is local to one Rust Host process. The separate Web and Desktop
development instances do not exclude each other from the same physical device;
run their device checks sequentially. Web displays devices attached to the DSH Host
machine, not automatically to the browser's machine. No continuous frames enter
the model context, and a panel does not itself grant agent vision/control.
