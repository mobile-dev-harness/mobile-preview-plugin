#!/usr/bin/env node

import { spawnSync } from 'node:child_process';
import { createHash } from 'node:crypto';
import { chmodSync, copyFileSync, existsSync, linkSync, lstatSync, mkdirSync, mkdtempSync,
  readFileSync, readdirSync, readlinkSync, rmSync, writeFileSync } from 'node:fs';
import { dirname, join, resolve } from 'node:path';
import { homedir } from 'node:os';
import { fileURLToPath, pathToFileURL } from 'node:url';

const ROOT = resolve(dirname(fileURLToPath(import.meta.url)), '..');
const HOST = 'native/darwin-arm64/mpp';
const ASSETS = 'native/android-arm64';
const PAYLOAD = ['index.js', 'client.js', 'src', 'cordis.patch.yml', 'README.md'];
const LEGAL = ['LICENSE', 'THIRD-PARTY-NOTICES.md', 'licenses/third-party'];
const HELP = `Usage: node scripts/package-preview.mjs --version VERSION [--output-dir PATH]

Build and pack a local macOS arm64 preview with its Rust host and Android arm64
assets (Android 10–17 / API 29–37). VERSION must be <source-version>-preview.<number>.
Requires installed Rust and NDK matching licenses/third-party/manifest.json,
Android build-tools and JDK; installs nothing.
Creates a private .tgz and SHA-256 checksum under target/packages by default.
Records the source commit, dirty state and source tree hash; rejects changes during the build.
Does not publish a package or GitHub Release.`;

const json = path => JSON.parse(readFileSync(path, 'utf8'));
const digest = path => createHash('sha256').update(readFileSync(path)).digest('hex');

function run(command, args, { trimOutput = true, ...options } = {}) {
  const result = spawnSync(command, args, { encoding: 'utf8', maxBuffer: 8 * 1024 * 1024,
    stdio: ['ignore', 'pipe', 'pipe'], ...options });
  if (result.error || result.status !== 0) {
    throw new Error(`${command} failed: ${result.error?.message ?? result.stderr ?? result.status}`);
  }
  const output = result.stdout ?? '';
  return trimOutput ? output.trim() : output;
}

function copyTree(source, target, sourceFiles) {
  const stat = lstatSync(source);
  if (stat.isDirectory()) {
    mkdirSync(target, { recursive: true });
    for (const name of readdirSync(source)) copyTree(join(source, name), join(target, name), sourceFiles);
  } else if (stat.isFile()) {
    if (sourceFiles && !sourceFiles.has(resolve(source))) {
      throw new Error(`Package source is outside source provenance or ignored by Git: ${source}`);
    }
    mkdirSync(dirname(target), { recursive: true });
    copyFileSync(source, target);
    chmodSync(target, stat.mode & 0o111 ? 0o755 : 0o644);
  } else throw new Error(`Package inputs must be ordinary files or directories: ${source}`);
}

function validateVersion(sourceVersion, version) {
  if (!/^\d+\.\d+\.\d+$/.test(sourceVersion) || typeof version !== 'string'
    || !version.startsWith(`${sourceVersion}-preview.`)
    || !/^(0|[1-9]\d*)$/.test(version.slice(`${sourceVersion}-preview.`.length))) {
    throw new Error(`Preview version must be ${sourceVersion}-preview.<number>.`);
  }
}

function sourcePaths(repository) {
  return run('git', ['ls-files', '-z', '--cached', '--others', '--exclude-standard'],
    { cwd: repository, trimOutput: false }).split('\0').filter(Boolean);
}

/** Describe actual checkout contents, including uncommitted sources but not ignored local state. */
export function readSourceState(repository) {
  const git = args => run('git', args, { cwd: repository });
  const commit = git(['rev-parse', '--verify', 'HEAD']);
  const dirty = git(['status', '--porcelain=v1', '--untracked-files=all']).length > 0;
  const tree = createHash('sha256');
  for (const path of [...new Set(sourcePaths(repository))].sort()) {
    const absolute = join(repository, path);
    const stat = lstatSync(absolute, { throwIfNoEntry: false });
    let entry;
    if (!stat) entry = [path, 'missing'];
    else if (stat.isSymbolicLink()) entry = [path, 'symlink', readlinkSync(absolute)];
    else if (stat.isFile()) entry = [path, stat.mode & 0o111 ? 'executable' : 'file', digest(absolute)];
    else throw new Error(`Unsupported source entry: ${path}`);
    tree.update(JSON.stringify(entry) + '\n');
  }
  return { commit, dirty, treeSha256: tree.digest('hex') };
}

export function assertSourceUnchanged(repository, source) {
  if (JSON.stringify(readSourceState(repository)) !== JSON.stringify(source)) {
    throw new Error('Source changed during the build; retry from a stable checkout. No artifact was packed.');
  }
}

export function verifyLicenseInventory(repository) {
  const base = join(repository, 'licenses/third-party');
  const inventory = json(join(base, 'manifest.json'));
  if (inventory.cargoLockSha256 !== digest(join(repository, 'Cargo.lock'))) {
    throw new Error('Third-party license inventory does not match Cargo.lock; refresh the notices before packaging.');
  }
  const invalid = () => new Error('Third-party license inventory has incomplete or duplicate crate coverage; refresh the notices before packaging.');
  const required = new Set();
  for (const target of ['aarch64-apple-darwin', 'aarch64-linux-android']) {
    for (const role of ['runtime', 'build']) {
      const entries = inventory.targets?.[target]?.[role];
      if (!Array.isArray(entries) || entries.some(entry => typeof entry !== 'string')
        || new Set(entries).size !== entries.length) throw invalid();
      for (const entry of entries) required.add(entry);
    }
  }
  const covered = new Set();
  if (!Array.isArray(inventory.crates)) throw invalid();
  for (const crate of inventory.crates) {
    const key = `${crate.name}@${crate.version}`;
    if (!required.has(key) || covered.has(key) || !Array.isArray(crate.licenseFiles)
      || crate.licenseFiles.length === 0) throw invalid();
    covered.add(key);
  }
  if (covered.size !== required.size) throw invalid();
  const files = [...inventory.crates.flatMap(crate => crate.licenseFiles),
    ...inventory.rustStandardLibrary.files, ...inventory.androidNdkRuntimeNotices.files];
  for (const file of files) {
    if (typeof file.path !== 'string' || file.path.startsWith('/') || file.path.split('/').includes('..')
      || !lstatSync(join(base, file.path)).isFile() || digest(join(base, file.path)) !== file.sha256) {
      throw new Error(`Third-party license file is missing or changed: ${file.path}`);
    }
  }
  return inventory;
}

/** Stage a self-contained package without copying a developer's build tree. */
export function stagePreview({ repository, pluginDir, hostExecutable, deviceAssets, licenseDir, stageDir, version, source }) {
  const plugin = json(join(pluginDir, 'package.json'));
  validateVersion(plugin.version, version);
  if (!source || !/^(?:[a-f0-9]{40}|[a-f0-9]{64})$/.test(source.commit) || typeof source.dirty !== 'boolean'
    || !/^[a-f0-9]{64}$/.test(source.treeSha256)) {
    throw new Error('Valid source provenance (commit, dirty, treeSha256) is required.');
  }
  if (typeof repository !== 'string') throw new Error('A source repository is required to validate package source provenance.');
  const sourceFiles = new Set(sourcePaths(repository).map(path => resolve(repository, path)));
  if (!sourceFiles.has(resolve(pluginDir, 'package.json'))) {
    throw new Error('Plugin package.json is outside source provenance or ignored by Git.');
  }
  if (existsSync(stageDir)) throw new Error('Package staging directory must not already exist.');
  for (const path of [hostExecutable, join(deviceAssets, 'bootstrap.jar'), join(deviceAssets, 'libmpp_android_device.so'),
    join(licenseDir, 'LICENSE'), join(licenseDir, 'THIRD-PARTY-NOTICES.md')]) {
    if (!lstatSync(path).isFile() || lstatSync(path).size === 0) throw new Error(`Missing ordinary nonempty package file: ${path}`);
  }
  const licensePath = join(licenseDir, 'licenses/third-party');
  if (!lstatSync(licensePath).isDirectory() || readdirSync(licensePath).length === 0) {
    throw new Error('Third-party license files are required.');
  }
  verifyLicenseInventory(licenseDir);
  mkdirSync(stageDir, { recursive: true });
  for (const name of PAYLOAD) copyTree(join(pluginDir, name), join(stageDir, name), sourceFiles);
  for (const name of LEGAL) copyTree(join(licenseDir, name), join(stageDir, name), sourceFiles);
  copyTree(hostExecutable, join(stageDir, HOST));
  chmodSync(join(stageDir, HOST), 0o755);
  for (const name of ['bootstrap.jar', 'libmpp_android_device.so']) {
    copyTree(join(deviceAssets, name), join(stageDir, ASSETS, name));
  }
  // These are locally installable preview artifacts, not an npm publication.
  const manifest = { ...plugin, version, private: true, os: ['darwin'], cpu: ['arm64'],
    files: [...PAYLOAD, ...LEGAL, 'native', 'runtime-manifest.json'] };
  delete manifest.scripts;
  writeFileSync(join(stageDir, 'package.json'), JSON.stringify(manifest, null, 2) + '\n');
  const runtime = {
    formatVersion: 1, version, hostVersion: plugin.version, source,
    host: { platform: 'darwin', arch: 'arm64', profile: 'release', minimumMacOS: '14.0' },
    android: { minApi: 29, supportedApis: [29, 30, 31, 32, 33, 34, 35, 36, 37], abi: 'arm64-v8a', profile: 'release' },
    dsh: { version: '0.2.1-alpha.1', sourceCommit: '5badb15009ae1756c3afe0ae0cef1faafc290ccc' },
    files: [HOST, `${ASSETS}/bootstrap.jar`, `${ASSETS}/libmpp_android_device.so`]
      .map(path => ({ path, sha256: digest(join(stageDir, path)) })),
  };
  writeFileSync(join(stageDir, 'runtime-manifest.json'), JSON.stringify(runtime, null, 2) + '\n');
  return manifest;
}

/** Pack only staged files; installation never compiles or downloads native code. */
export function packPreview({ stageDir, outputDir, npm = 'npm' }) {
  const manifest = json(join(stageDir, 'package.json'));
  const filename = `mobile-dev-harness-dsh-mobile-preview-${manifest.version}-darwin-arm64.tgz`;
  const destination = join(outputDir, filename);
  if (existsSync(destination) || existsSync(`${destination}.sha256`)) {
    throw new Error('Preview artifact already exists; choose another version or output directory.');
  }
  mkdirSync(outputDir, { recursive: true });
  const scratch = mkdtempSync(join(outputDir, '.mpp-pack-'));
  try {
    const result = JSON.parse(run(npm, ['pack', '--json', '--ignore-scripts', '--offline',
      '--pack-destination', scratch, '--cache', join(scratch, 'cache')], { cwd: stageDir }));
    if (result.length !== 1 || result[0].name !== manifest.name || result[0].version !== manifest.version) {
      throw new Error('npm did not pack the expected preview package.');
    }
    const paths = new Set(result[0].files.map(file => file.path));
    const licenses = readdirSync(join(stageDir, 'licenses/third-party'), { recursive: true })
      .map(path => `licenses/third-party/${path}`)
      .filter(path => lstatSync(join(stageDir, path)).isFile());
    for (const path of ['index.js', 'client.js', 'src/host/runtime.mjs', 'cordis.patch.yml',
      'LICENSE', 'THIRD-PARTY-NOTICES.md', ...licenses,
      'runtime-manifest.json', HOST, `${ASSETS}/bootstrap.jar`, `${ASSETS}/libmpp_android_device.so`]) {
      if (!paths.has(path)) throw new Error(`Packed preview is missing ${path}.`);
    }
    const packed = join(scratch, result[0].filename);
    const hash = digest(packed);
    const checksum = join(scratch, 'preview.sha256');
    writeFileSync(checksum, `${hash}  ${filename}\n`, { flag: 'wx' });
    // Both links are exclusive: another pack cannot replace either completed file.
    linkSync(packed, destination);
    try { linkSync(checksum, `${destination}.sha256`); }
    catch (error) {
      rmSync(destination, { force: true });
      throw error;
    }
    return { path: destination, sha256: hash, version: manifest.version };
  } finally { rmSync(scratch, { recursive: true, force: true }); }
}

async function main(argv) {
  if (argv.length === 1 && ['--help', '-h'].includes(argv[0])) { console.log(HELP); return; }
  const source = json(join(ROOT, 'packages/dsh-plugin/package.json'));
  const settings = { outputDir: join(ROOT, 'target/packages') };
  const seen = new Set();
  while (argv.length) {
    const flag = argv.shift();
    if (!['--output-dir', '--version'].includes(flag) || seen.has(flag)) throw new Error(`Unknown or repeated option: ${flag}`);
    seen.add(flag);
    const value = argv.shift();
    if (!value || value.startsWith('--')) throw new Error(`${flag} requires a value.`);
    if (flag === '--output-dir') settings.outputDir = resolve(value);
    else settings.version = value;
  }
  if (!settings.version) throw new Error('--version is required; choose an explicit preview version.');
  validateVersion(source.version, settings.version);
  if (process.platform !== 'darwin' || process.arch !== 'arm64') throw new Error('The preview package must be built on macOS Apple Silicon.');
  const provenance = readSourceState(ROOT);
  const buildDir = join(ROOT, 'target/preview-build');
  const inventory = verifyLicenseInventory(ROOT);
  const sdk = process.env.ANDROID_HOME ?? process.env.ANDROID_SDK_ROOT ?? join(homedir(), 'Library/Android/sdk');
  const ndk = process.env.ANDROID_NDK_HOME ?? join(sdk, 'ndk', inventory.androidNdkRuntimeNotices.version);
  const env = { ...process.env, ANDROID_NDK_HOME: ndk, CARGO_TARGET_DIR: buildDir,
    MACOSX_DEPLOYMENT_TARGET: '14.0',
    MPP_BUILD_PROFILE: 'release', RUSTUP_AUTO_INSTALL: '0' };
  const rust = run(process.env.RUSTC ?? 'rustc', ['--version'], { cwd: ROOT, env }).split(' ')[1];
  const ndkVersion = readFileSync(join(ndk, 'source.properties'), 'utf8').match(/^Pkg.Revision\s*=\s*(\S+)/m)?.[1];
  if (rust !== inventory.rustStandardLibrary.version || ndkVersion !== inventory.androidNdkRuntimeNotices.version) {
    throw new Error(`License inventory requires Rust ${inventory.rustStandardLibrary.version} and NDK ${inventory.androidNdkRuntimeNotices.version}; select matching installed toolchains or refresh the notices.`);
  }
  console.error('Building the release host and Android runtime using installed toolchains...');
  run(process.env.CARGO ?? 'cargo', ['build', '--locked', '--release', '--package', 'mpp-host',
    '--target', 'aarch64-apple-darwin', '--target-dir', buildDir], { cwd: ROOT, env, stdio: 'inherit' });
  run('bash', [join(ROOT, 'scripts/build-android-device.sh')], { cwd: ROOT, env, stdio: 'inherit' });
  const hostExecutable = join(buildDir, 'aarch64-apple-darwin/release/mpp');
  if (run(hostExecutable, ['--version']) !== `mpp ${source.version}`) throw new Error('Rust host and plugin source versions do not match.');
  mkdirSync(settings.outputDir, { recursive: true });
  const scratch = mkdtempSync(join(buildDir, '.mpp-stage-'));
  try {
    const stageDir = join(scratch, 'package');
    stagePreview({ repository: ROOT, pluginDir: join(ROOT, 'packages/dsh-plugin'), hostExecutable,
      deviceAssets: join(buildDir, 'android-device'), licenseDir: ROOT, stageDir,
      version: settings.version, source: provenance });
    assertSourceUnchanged(ROOT, provenance);
    console.log(JSON.stringify(packPreview({ stageDir, outputDir: settings.outputDir }), null, 2));
  } finally { rmSync(scratch, { recursive: true, force: true }); }
}

if (process.argv[1] && import.meta.url === pathToFileURL(resolve(process.argv[1])).href) {
  main(process.argv.slice(2)).catch(error => { console.error(`package-preview: ${error.message}`); process.exitCode = 1; });
}
