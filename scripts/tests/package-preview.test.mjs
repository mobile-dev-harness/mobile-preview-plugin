import assert from 'node:assert/strict';
import { spawnSync } from 'node:child_process';
import { createHash } from 'node:crypto';
import { cpSync, existsSync, mkdirSync, mkdtempSync, readFileSync, readdirSync,
  realpathSync, rmSync, statSync, symlinkSync, writeFileSync } from 'node:fs';
import { tmpdir } from 'node:os';
import { dirname, join, resolve } from 'node:path';
import { fileURLToPath, pathToFileURL } from 'node:url';
import test from 'node:test';
import { assertSourceUnchanged, packPreview, readSourceState, stagePreview, verifyLicenseInventory } from '../package-preview.mjs';

const plugin = resolve(dirname(fileURLToPath(import.meta.url)), '../../packages/dsh-plugin');
const hash = path => createHash('sha256').update(readFileSync(path)).digest('hex');
const source = { commit: 'a'.repeat(40), dirty: true, treeSha256: 'b'.repeat(64) };

function fixture(t) {
  const root = realpathSync(mkdtempSync(join(tmpdir(), 'mpp package ')));
  t.after(() => rmSync(root, { recursive: true, force: true }));
  const pluginDir = join(root, 'source plugin');
  cpSync(plugin, pluginDir, { recursive: true });
  writeFileSync(join(pluginDir, 'README.md'), 'Local preview fixture.\n');
  writeFileSync(join(pluginDir, '.env'), 'secret-do-not-package');
  const hostExecutable = join(root, 'build/mpp');
  const deviceAssets = join(root, 'build/android-device');
  mkdirSync(deviceAssets, { recursive: true });
  writeFileSync(hostExecutable, '#!/bin/sh\nprintf "mpp 0.1.0\\n"\n', { mode: 0o755 });
  for (const name of ['bootstrap.jar', 'libmpp_android_device.so']) writeFileSync(join(deviceAssets, name), name);
  const licenseDir = join(root, 'legal');
  mkdirSync(join(licenseDir, 'licenses/third-party'), { recursive: true });
  writeFileSync(join(licenseDir, 'LICENSE'), 'Project license fixture.\n');
  writeFileSync(join(licenseDir, 'THIRD-PARTY-NOTICES.md'), 'Third-party notices fixture.\n');
  writeFileSync(join(licenseDir, 'licenses/third-party/dependency.txt'), 'Dependency license fixture.\n');
  writeFileSync(join(licenseDir, 'Cargo.lock'), 'Dependency lock fixture.\n');
  writeFileSync(join(licenseDir, 'licenses/third-party/manifest.json'), JSON.stringify({
    cargoLockSha256: hash(join(licenseDir, 'Cargo.lock')),
    targets: {
      'aarch64-apple-darwin': { runtime: ['dependency@1.0.0'], build: [] },
      'aarch64-linux-android': { runtime: ['dependency@1.0.0'], build: [] },
    },
    crates: [{ name: 'dependency', version: '1.0.0',
      licenseFiles: [{ path: 'dependency.txt', sha256: hash(join(licenseDir, 'licenses/third-party/dependency.txt')) }] }],
    rustStandardLibrary: { version: '1.88.0', files: [] },
    androidNdkRuntimeNotices: { version: '26.1.10909125', files: [] },
  }));
  writeFileSync(join(root, '.gitignore'), '.env\n');
  const initialized = spawnSync('git', ['init', '--quiet', root], { encoding: 'utf8' });
  assert.equal(initialized.status, 0, initialized.stderr);
  return { root, repository: root, pluginDir, hostExecutable, deviceAssets, licenseDir, source,
    stageDir: join(root, 'staging'), version: '0.1.0-preview.1' };
}

function racingPack(f, outputDir, collision) {
  const destination = join(outputDir, `mobile-dev-harness-dsh-mobile-preview-${f.version}-darwin-arm64.tgz`);
  const npm = join(f.root, 'racing-npm');
  const files = ['index.js', 'client.js', 'src/host/runtime.mjs', 'cordis.patch.yml',
    'LICENSE', 'THIRD-PARTY-NOTICES.md', 'licenses/third-party/dependency.txt', 'licenses/third-party/manifest.json',
    'runtime-manifest.json', 'native/darwin-arm64/mpp',
    'native/android-arm64/bootstrap.jar', 'native/android-arm64/libmpp_android_device.so'];
  writeFileSync(npm, `#!/usr/bin/env node
const { writeFileSync } = require('node:fs');
const { join } = require('node:path');
const scratch = process.argv[process.argv.indexOf('--pack-destination') + 1];
writeFileSync(join(scratch, 'candidate.tgz'), 'candidate archive');
writeFileSync(${JSON.stringify(collision === 'archive' ? destination : `${destination}.sha256`)}, 'competing artifact');
process.stdout.write(JSON.stringify([{ name: '@mobile-dev-harness/dsh-mobile-preview',
  version: ${JSON.stringify(f.version)}, filename: 'candidate.tgz',
  files: ${JSON.stringify(files.map(path => ({ path })))} }]));
`, { mode: 0o755 });
  return { npm, destination };
}

test('packed package relocates with complete runtime paths, permissions, hashes and no build scripts', async t => {
  const f = fixture(t);
  const tracked = spawnSync('git', ['add', 'source plugin/src/host/runtime.mjs'], { cwd: f.root, encoding: 'utf8' });
  assert.equal(tracked.status, 0, tracked.stderr);
  const manifest = stagePreview(f);
  assert.equal(manifest.private, true);
  assert.equal(manifest.scripts, undefined);
  assert.deepEqual(manifest.os, ['darwin']);
  assert.deepEqual(manifest.cpu, ['arm64']);
  const outputDir = join(f.root, 'artifacts');
  const packed = packPreview({ stageDir: f.stageDir, outputDir });
  assert.equal(packed.sha256, hash(packed.path));
  assert.match(readFileSync(`${packed.path}.sha256`, 'utf8'), new RegExp(`^${packed.sha256}  `));
  assert(!readdirSync(outputDir).some(name => name.startsWith('.mpp-pack-')));
  const installed = join(f.root, 'new installation');
  mkdirSync(installed);
  const extraction = spawnSync('tar', ['-xzf', packed.path, '-C', installed], { encoding: 'utf8' });
  assert.equal(extraction.status, 0, extraction.stderr);
  rmSync(f.pluginDir, { recursive: true });
  rmSync(dirname(f.hostExecutable), { recursive: true });
  rmSync(f.stageDir, { recursive: true });
  const packageRoot = join(installed, 'package');
  assert(!existsSync(join(packageRoot, 'tests')));
  assert(!existsSync(join(packageRoot, '.env')));
  assert.equal(readFileSync(join(packageRoot, 'LICENSE'), 'utf8'), 'Project license fixture.\n');
  assert.equal(readFileSync(join(packageRoot, 'THIRD-PARTY-NOTICES.md'), 'utf8'), 'Third-party notices fixture.\n');
  assert.equal(readFileSync(join(packageRoot, 'licenses/third-party/dependency.txt'), 'utf8'), 'Dependency license fixture.\n');
  const runtime = JSON.parse(readFileSync(join(packageRoot, 'runtime-manifest.json')));
  assert.equal(runtime.version, f.version);
  assert.equal(runtime.hostVersion, '0.1.0');
  assert.deepEqual(runtime.source, source);
  assert.deepEqual(runtime.android, { minApi: 29, supportedApis: [29, 30, 31, 32, 33, 34, 35, 36, 37],
    abi: 'arm64-v8a', profile: 'release' });
  assert(!existsSync(join(packageRoot, 'native/android-api32-arm64')));
  for (const file of runtime.files) assert.equal(hash(join(packageRoot, file.path)), file.sha256);
  const { resolveRuntime } = await import(pathToFileURL(join(packageRoot, 'src/host/runtime.mjs')).href);
  const settings = resolveRuntime({}, { platform: 'darwin', arch: 'arm64' });
  assert.equal(settings.executable, join(packageRoot, 'native/darwin-arm64/mpp'));
  assert.equal(settings.deviceAssets, join(packageRoot, 'native/android-arm64'));
  assert(statSync(settings.executable).mode & 0o111);
  assert.equal(spawnSync(settings.executable, ['--version'], { encoding: 'utf8' }).stdout.trim(), 'mpp 0.1.0');
  if (process.platform === 'darwin' && process.arch === 'arm64') {
    const { resolveConfig } = await import(pathToFileURL(join(packageRoot, 'index.js')).href);
    assert.equal(resolveConfig().executable, settings.executable);
  }
});

test('missing, empty and symlinked native inputs cannot be packed', t => {
  for (const replacement of ['missing', 'empty', 'symlink']) {
    const f = fixture(t);
    const library = join(f.deviceAssets, 'libmpp_android_device.so');
    rmSync(library);
    if (replacement === 'empty') writeFileSync(library, '');
    if (replacement === 'symlink') symlinkSync(f.hostExecutable, library);
    assert.throws(() => stagePreview(f));
    assert(!existsSync(f.stageDir));
  }
});

test('ignored package sources and legal files cannot escape source provenance', t => {
  for (const path of ['source plugin/src/.env', 'source plugin/src/ignored.mjs',
    'source plugin/package.json', 'legal/licenses/third-party/dependency.txt']) {
    const f = fixture(t);
    writeFileSync(join(f.root, '.gitignore'), `.env\n/${path}\n`);
    const git = (...args) => {
      const result = spawnSync('git', args, { cwd: f.root, encoding: 'utf8' });
      assert.equal(result.status, 0, result.stderr);
    };
    git('add', '.');
    git('-c', 'user.name=Package test', '-c', 'user.email=test@example.invalid',
      '-c', 'commit.gpgsign=false', 'commit', '--quiet', '-m', 'Create provenance fixture');
    const before = readSourceState(f.root);
    if (path.startsWith('source plugin/src/')) writeFileSync(join(f.root, path), 'must not ship');
    assert.deepEqual(readSourceState(f.root), before);
    assert.equal(before.dirty, false);
    assert.throws(() => stagePreview({ ...f, source: before }), /source provenance/);
  }
});

test('preview versions stay tied to the source version and cannot masquerade as stable releases', t => {
  const f = fixture(t);
  for (const version of [undefined, '0.1.0', '0.2.0-preview.1', '0.1.0-preview.01', '0.1.0-preview.', '../bad']) {
    assert.throws(() => stagePreview({ ...f, version }), /Preview version/);
    assert(!existsSync(f.stageDir));
  }
});

test('the CLI requires an explicit valid preview version before starting a build', () => {
  const script = resolve(dirname(fileURLToPath(import.meta.url)), '../package-preview.mjs');
  for (const args of [[], ['--version', '0.1.0'], ['--version', '0.2.0-preview.1']]) {
    const result = spawnSync(process.execPath, [script, ...args], { encoding: 'utf8' });
    assert.equal(result.status, 1);
    assert.match(result.stderr, /--version is required|Preview version/);
    assert.doesNotMatch(result.stderr, /Building the release host/);
  }
});

test('packages require source provenance and their complete license payload', t => {
  for (const invalid of [undefined, { ...source, commit: 'unknown' }, { ...source, commit: 'a'.repeat(41) }, { ...source, dirty: 'false' },
    { ...source, treeSha256: 'unknown' }]) {
    const f = fixture(t);
    assert.throws(() => stagePreview({ ...f, source: invalid }), /source provenance/);
    assert(!existsSync(f.stageDir));
  }
  for (const missing of ['LICENSE', 'THIRD-PARTY-NOTICES.md', 'licenses/third-party']) {
    const f = fixture(t);
    rmSync(join(f.licenseDir, missing), { recursive: true });
    assert.throws(() => stagePreview(f));
    assert(!existsSync(f.stageDir));
  }
});

test('source provenance detects tracked and untracked changes without including ignored local state', t => {
  const f = fixture(t);
  const repository = join(f.root, 'repository');
  mkdirSync(repository);
  const git = (...args) => {
    const result = spawnSync('git', args, { cwd: repository, encoding: 'utf8' });
    assert.equal(result.status, 0, result.stderr);
    return result.stdout.trim();
  };
  git('init', '--quiet');
  writeFileSync(join(repository, 'tracked.txt'), 'original');
  writeFileSync(join(repository, '.gitignore'), 'local-state/\n');
  git('add', '.');
  git('-c', 'user.name=Package test', '-c', 'user.email=test@example.invalid',
    '-c', 'commit.gpgsign=false', 'commit', '--quiet', '-m', 'Create package fixture');
  const clean = readSourceState(repository);
  assert.equal(clean.commit, git('rev-parse', 'HEAD'));
  assert.equal(clean.dirty, false);
  assert.match(clean.treeSha256, /^[a-f0-9]{64}$/);
  mkdirSync(join(repository, 'local-state'));
  writeFileSync(join(repository, 'local-state/secret'), 'must not affect provenance');
  assert.deepEqual(readSourceState(repository), clean);
  assertSourceUnchanged(repository, clean);
  writeFileSync(join(repository, 'tracked.txt'), 'changed');
  const dirty = readSourceState(repository);
  assert.equal(dirty.commit, clean.commit);
  assert.equal(dirty.dirty, true);
  assert.notEqual(dirty.treeSha256, clean.treeSha256);
  assert.throws(() => assertSourceUnchanged(repository, clean), /Source changed during the build/);
  writeFileSync(join(repository, 'tracked.txt'), 'changed again');
  assert.throws(() => assertSourceUnchanged(repository, dirty), /Source changed during the build/);
  writeFileSync(join(repository, 'tracked.txt'), 'original');
  writeFileSync(join(repository, ' untracked source .txt'), 'new source');
  const untracked = readSourceState(repository);
  assert.equal(untracked.dirty, true);
  assert.notEqual(untracked.treeSha256, clean.treeSha256);
  rmSync(join(repository, 'tracked.txt'));
  assert.notEqual(readSourceState(repository).treeSha256, untracked.treeSha256);
});

test('packaging rejects stale lockfile notices and changed or missing license text', t => {
  for (const changed of ['Cargo.lock', 'licenses/third-party/dependency.txt']) {
    const f = fixture(t);
    writeFileSync(join(f.licenseDir, changed), 'changed content');
    assert.throws(() => stagePreview(f), /license inventory|license file/);
    assert(!existsSync(f.stageDir));
  }
  const f = fixture(t);
  verifyLicenseInventory(f.licenseDir);
  rmSync(join(f.licenseDir, 'licenses/third-party/dependency.txt'));
  assert.throws(() => stagePreview(f));
  assert(!existsSync(f.stageDir));
});

test('license inventory requires complete, unique crate coverage and nonempty license files', t => {
  const changes = [
    inventory => { inventory.crates = []; },
    inventory => { inventory.crates[0].licenseFiles = []; },
    inventory => { inventory.crates.push(structuredClone(inventory.crates[0])); },
    inventory => { inventory.crates[0].name = 'unreferenced'; },
    inventory => { delete inventory.targets['aarch64-linux-android']; },
    inventory => { inventory.targets['aarch64-apple-darwin'].build = ['missing@1.0.0']; },
    inventory => { inventory.targets['aarch64-apple-darwin'].runtime.push('dependency@1.0.0'); },
  ];
  for (const change of changes) {
    const f = fixture(t);
    const path = join(f.licenseDir, 'licenses/third-party/manifest.json');
    const inventory = JSON.parse(readFileSync(path));
    change(inventory);
    writeFileSync(path, JSON.stringify(inventory));
    assert.throws(() => stagePreview(f), /license inventory/);
    assert(!existsSync(f.stageDir));
  }
});

test('removing a dependency and its license cannot hide an incomplete inventory', t => {
  const f = fixture(t);
  const path = join(f.licenseDir, 'licenses/third-party/manifest.json');
  const inventory = JSON.parse(readFileSync(path));
  inventory.crates = [];
  writeFileSync(path, JSON.stringify(inventory));
  rmSync(join(f.licenseDir, 'licenses/third-party/dependency.txt'));
  assert.throws(() => stagePreview(f), /license inventory/);
  assert(!existsSync(f.stageDir));
});

test('staging and completed artifacts are never silently overwritten', t => {
  const f = fixture(t);
  stagePreview(f);
  assert.throws(() => stagePreview(f), /must not already exist/);
  const outputDir = join(f.root, 'artifacts');
  const packed = packPreview({ stageDir: f.stageDir, outputDir });
  assert.throws(() => packPreview({ stageDir: f.stageDir, outputDir }), /already exists/);
  assert.equal(hash(packed.path), packed.sha256);
});

test('pack failure does not publish an incomplete tarball and removes temporary output', t => {
  const f = fixture(t);
  stagePreview(f);
  const outputDir = join(f.root, 'artifacts');
  assert.throws(() => packPreview({ stageDir: f.stageDir, outputDir, npm: '/missing-mpp-npm' }), /failed/);
  assert.deepEqual(readdirSync(outputDir), []);
});

test('an archive created while npm packs cannot be overwritten or removed', t => {
  const f = fixture(t);
  stagePreview(f);
  const outputDir = join(f.root, 'artifacts');
  const { npm, destination } = racingPack(f, outputDir, 'archive');
  assert.throws(() => packPreview({ stageDir: f.stageDir, outputDir, npm }), { code: 'EEXIST' });
  assert.equal(readFileSync(destination, 'utf8'), 'competing artifact');
  assert(!existsSync(`${destination}.sha256`));
  assert.deepEqual(readdirSync(outputDir), [destination.slice(outputDir.length + 1)]);
});

test('a checksum created while npm packs survives and only the new archive is rolled back', t => {
  const f = fixture(t);
  stagePreview(f);
  const outputDir = join(f.root, 'artifacts');
  const { npm, destination } = racingPack(f, outputDir, 'checksum');
  assert.throws(() => packPreview({ stageDir: f.stageDir, outputDir, npm }), { code: 'EEXIST' });
  assert.equal(readFileSync(`${destination}.sha256`, 'utf8'), 'competing artifact');
  assert(!existsSync(destination));
  assert.deepEqual(readdirSync(outputDir), [`${destination.slice(outputDir.length + 1)}.sha256`]);
});
