import assert from 'node:assert/strict';
import { chmodSync, copyFileSync, mkdirSync, mkdtempSync, realpathSync, renameSync, rmSync, symlinkSync, writeFileSync } from 'node:fs';
import { tmpdir } from 'node:os';
import { dirname, join } from 'node:path';
import { pathToFileURL } from 'node:url';
import test from 'node:test';
import { resolveRuntime } from '../src/host/runtime.mjs';

const supported = { platform: 'darwin', arch: 'arm64' };
const files = ['native/darwin-arm64/mpp', 'native/android-arm64/bootstrap.jar',
  'native/android-arm64/libmpp_android_device.so'];

function fixture(t) {
  const packageRoot = realpathSync(mkdtempSync(join(tmpdir(), 'mpp-runtime-')));
  t.after(() => rmSync(packageRoot, { recursive: true, force: true }));
  for (const [index, name] of files.entries()) {
    const path = join(packageRoot, name);
    mkdirSync(dirname(path), { recursive: true });
    writeFileSync(path, 'fixture', { mode: index === 0 ? 0o755 : 0o644 });
  }
  return packageRoot;
}

test('installed runtime resolves all assets from its own package location', async t => {
  const packageRoot = fixture(t);
  const modulePath = join(packageRoot, 'src/host/runtime.mjs');
  mkdirSync(dirname(modulePath), { recursive: true });
  copyFileSync(new URL('../src/host/runtime.mjs', import.meta.url), modulePath);
  const installed = await import(pathToFileURL(modulePath).href);
  assert.deepEqual(installed.resolveRuntime({}, supported), {
    executable: join(packageRoot, files[0]),
    deviceAssets: join(packageRoot, 'native/android-arm64'),
  });
  assert.notEqual(packageRoot, process.cwd());
});

test('bundled runtime reports unsupported host before inspecting files', () => {
  for (const [platform, arch] of [['linux', 'arm64'], ['darwin', 'x64'], ['win32', 'arm64']]) {
    assert.throws(() => resolveRuntime({}, { packageRoot: '/not-installed', platform, arch }), error =>
      error.code === 'UNSUPPORTED_HOST' && /macOS Apple Silicon/.test(error.message)
      && /absolute executable path/.test(error.hint));
  }
});

test('an incomplete package fails on each missing bundled file with reinstall guidance', t => {
  for (const file of files) {
    const packageRoot = fixture(t);
    rmSync(join(packageRoot, file));
    assert.throws(() => resolveRuntime({}, { ...supported, packageRoot }), error =>
      error.code === 'INCOMPLETE_PACKAGE' && /Reinstall/.test(error.hint)
      && !error.message.includes(packageRoot));
  }
});

test('an old API 32 bundle cannot silently replace the version-neutral Android runtime', t => {
  const packageRoot = fixture(t);
  renameSync(join(packageRoot, 'native/android-arm64'), join(packageRoot, 'native/android-api32-arm64'));
  assert.throws(() => resolveRuntime({}, { ...supported, packageRoot }), { code: 'INCOMPLETE_PACKAGE' });
});

test('bundled files must be regular files, not directories or symlinks', t => {
  for (const replacement of ['directory', 'symlink']) {
    const packageRoot = fixture(t);
    const library = join(packageRoot, files[2]);
    rmSync(library);
    if (replacement === 'directory') mkdirSync(library);
    else symlinkSync(join(packageRoot, files[1]), library);
    assert.throws(() => resolveRuntime({}, { ...supported, packageRoot }), { code: 'INCOMPLETE_PACKAGE' });
  }
});

test('bundled host requires execute permission and all bundled files require read permission', {
  skip: process.platform === 'win32' || process.getuid?.() === 0,
}, t => {
  const packageRoot = fixture(t);
  const host = join(packageRoot, files[0]);
  chmodSync(host, 0o644);
  assert.throws(() => resolveRuntime({}, { ...supported, packageRoot }), { code: 'INCOMPLETE_PACKAGE' });
  chmodSync(host, 0o755);
  for (const file of files) {
    const path = join(packageRoot, file);
    chmodSync(path, 0o111);
    assert.throws(() => resolveRuntime({}, { ...supported, packageRoot }), { code: 'INCOMPLETE_PACKAGE' });
    chmodSync(path, file === files[0] ? 0o755 : 0o644);
  }
});

test('explicit development host bypasses the bundle and preserves discovery-only mode', () => {
  const options = { packageRoot: '/not-installed', platform: 'linux', arch: 'x64' };
  assert.deepEqual(resolveRuntime({ executable: '/development/mpp' }, options), {
    executable: '/development/mpp',
  });
  assert.deepEqual(resolveRuntime({ executable: '/development/mpp', deviceAssets: '/development/assets' }, options), {
    executable: '/development/mpp', deviceAssets: '/development/assets',
  });
});

test('explicit asset directory overrides bundled assets when using the bundled host', t => {
  const packageRoot = fixture(t);
  const deviceAssets = join(fixture(t), 'native/android-arm64');
  rmSync(join(packageRoot, 'native/android-arm64'), { recursive: true });
  assert.deepEqual(resolveRuntime({ deviceAssets }, { ...supported, packageRoot }), {
    executable: join(packageRoot, files[0]), deviceAssets,
  });
  rmSync(join(deviceAssets, 'bootstrap.jar'));
  assert.throws(() => resolveRuntime({ deviceAssets }, { ...supported, packageRoot }), error =>
    error.code === 'INVALID_CONFIG' && /deviceAssets/.test(error.hint));
});

test('runtime selection never reads ambient MPP path overrides', t => {
  const packageRoot = fixture(t);
  const previous = [process.env.MPP_EXECUTABLE, process.env.MPP_DEVICE_ASSETS];
  t.after(() => {
    for (const [index, name] of ['MPP_EXECUTABLE', 'MPP_DEVICE_ASSETS'].entries()) {
      if (previous[index] === undefined) delete process.env[name];
      else process.env[name] = previous[index];
    }
  });
  process.env.MPP_EXECUTABLE = '/ambient/mpp';
  process.env.MPP_DEVICE_ASSETS = '/ambient/assets';
  assert.deepEqual(resolveRuntime({}, { ...supported, packageRoot }), {
    executable: join(packageRoot, files[0]), deviceAssets: join(packageRoot, 'native/android-arm64'),
  });
});
