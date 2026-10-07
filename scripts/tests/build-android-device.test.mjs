import assert from 'node:assert/strict';
import { spawnSync } from 'node:child_process';
import { copyFileSync, existsSync, mkdirSync, mkdtempSync, readFileSync,
  readdirSync, realpathSync, rmSync, writeFileSync } from 'node:fs';
import { tmpdir } from 'node:os';
import { dirname, join, resolve } from 'node:path';
import { fileURLToPath } from 'node:url';
import test from 'node:test';

const script = resolve(dirname(fileURLToPath(import.meta.url)), '../build-android-device.sh');

function fixture() {
  const root = realpathSync(mkdtempSync(join(tmpdir(), 'mpp android build ')));
  const sdk = join(root, 'Android SDK');
  const jdk = join(root, 'Java 17');
  const tools = join(root, 'fake tools');
  const trace = join(root, 'trace.jsonl');
  const stdlib = join(root, 'rust std');
  for (const directory of ['scripts', 'android-bootstrap/dev/mpp', 'rust std', 'fake tools', 'Java 17/bin']) {
    mkdirSync(join(root, directory), { recursive: true });
  }
  copyFileSync(script, join(root, 'scripts/build-android-device.sh'));
  writeFileSync(join(root, 'android-bootstrap/dev/mpp/Bootstrap.java'), 'package dev.mpp; public final class Bootstrap {}');
  writeFileSync(join(stdlib, 'libstd-test.rlib'), 'installed target');
  writeFileSync(join(root, 'Cargo.toml'), '[workspace]\n');
  function tool(path, name, body) {
    mkdirSync(dirname(path), { recursive: true });
    writeFileSync(path, `#!${process.execPath}\nconst fs=require('node:fs'),path=require('node:path');
const args=process.argv.slice(2);fs.appendFileSync(process.env.MPP_TEST_TRACE,JSON.stringify({tool:${JSON.stringify(name)},args,autoInstall:process.env.RUSTUP_AUTO_INSTALL,linker:process.env.CARGO_TARGET_AARCH64_LINUX_ANDROID_LINKER,androidLinker:process.env.MPP_ANDROID_LINKER,rustflags:process.env.RUSTFLAGS,encodedRustflags:process.env.CARGO_ENCODED_RUSTFLAGS,targetRustflags:process.env.CARGO_TARGET_AARCH64_LINUX_ANDROID_RUSTFLAGS})+'\\n');
${body}\n`, { mode: 0o755 });
  }
  tool(join(tools, 'rustc'), 'rustc', `console.log(${JSON.stringify(stdlib)});`);
  tool(join(tools, 'cargo'), 'cargo', `if(process.env.MPP_TEST_CARGO_FAIL)process.exit(7);
const dir=args[args.indexOf('--target-dir')+1];const output=path.join(dir,'aarch64-linux-android',args.includes('--release')?'release':'debug');
require('node:child_process').execFileSync(process.env.CARGO_TARGET_AARCH64_LINUX_ANDROID_LINKER,['-shared','-Wl,-z,max-page-size=4096','-o',path.join(output,'libmpp_android_device.so')]);
fs.mkdirSync(path.join(output,'examples'),{recursive:true});fs.writeFileSync(path.join(output,'libmpp_android_device.so'),'native-library');fs.writeFileSync(path.join(output,'examples/codec_probe'),'codec-probe');`);
  tool(join(jdk, 'bin/javac'), 'javac', `if(args[0]==='-version'){console.log('javac '+(process.env.MPP_TEST_JAVAC_VERSION||'17.0.18'));process.exit(0);}
const dir=path.join(args[args.indexOf('-d')+1],'dev/mpp');fs.mkdirSync(dir,{recursive:true});fs.writeFileSync(path.join(dir,'Bootstrap.class'),'class');fs.writeFileSync(path.join(dir,'Bootstrap$Nested.class'),'class');`);
  tool(join(jdk, 'bin/jar'), 'jar', `console.log(process.env.MPP_TEST_BAD_DEX?'other.txt':'classes.dex');`);
  const prebuilt = process.platform === 'darwin' ? 'darwin-x86_64' : 'linux-x86_64';
  const ndk = join(sdk, 'ndk/26.1.10909125');
  const ndkBin = join(ndk, `toolchains/llvm/prebuilt/${prebuilt}/bin`);
  tool(join(ndkBin, 'aarch64-linux-android29-clang'), 'clang', `if(args[0]==='--version')console.log('Android clang');`);
  tool(join(ndkBin, 'llvm-readelf'), 'readelf', `if(process.env.MPP_TEST_READELF_FAIL)process.exit(9);
console.log(process.env.MPP_TEST_ELF_HEADERS ?? 'Program Headers:\\n  Type Offset VirtAddr PhysAddr FileSiz MemSiz Flg Align\\n  LOAD 0x000000 0x000000 0x000000 0x018e58 0x018e58 R 0x4000\\n  LOAD 0x018e60 0x01ce60 0x01ce60 0x009330 0x009330 R E 0x4000\\n  LOAD 0x022190 0x02a190 0x02a190 0x000500 0x003e70 RW 0x4000');`);
  for (const version of ['33.0.1', '36.0.0', '37.0.0-rc1']) {
    tool(join(sdk, 'build-tools', version, 'd8'), `d8-${version}`, `if(args[0]==='--version'){console.log('D8 8.0.0');process.exit(0);}
fs.writeFileSync(args[args.indexOf('--output')+1],'dex-archive');`);
  }
  for (const version of ['28', '29', '30', '32', '34', '37.0']) {
    const directory = join(sdk, 'platforms', `android-${version}`);
    mkdirSync(directory, { recursive: true });
    writeFileSync(join(directory, 'android.jar'), 'android-api');
  }
  const env = { ...process.env, ANDROID_HOME: sdk, ANDROID_NDK_HOME: ndk,
    ANDROID_NDK_LATEST_HOME: '', JAVA_HOME: jdk, CARGO: join(tools, 'cargo'),
    RUSTC: join(tools, 'rustc'), CARGO_TARGET_DIR: join(root, 'build output'),
    MPP_BUILD_TOOLS: '', MPP_ANDROID_JAR: '', MPP_BUILD_PROFILE: '', MPP_TEST_TRACE: trace };
  return {
    root, sdk, jdk, ndkBin, env,
    run(extra = {}, args = []) {
      return spawnSync('bash', [join(root, 'scripts/build-android-device.sh'), ...args],
        { cwd: tmpdir(), env: { ...env, ...extra }, encoding: 'utf8', timeout: 30_000 });
    },
    calls() { return existsSync(trace) ? readFileSync(trace, 'utf8').trim().split('\n').map(JSON.parse) : []; },
    close() { rmSync(root, { recursive: true, force: true }); },
  };
}

test('packages separate bootstrap/library assets with Java 8 and min API 29 through paths containing spaces', () => {
  const f = fixture();
  try {
    const result = f.run();
    assert.equal(result.status, 0, result.stderr);
    const target = f.env.CARGO_TARGET_DIR;
    assert.deepEqual(result.stdout.trim().split('\n'), [join(target, 'android-device/bootstrap.jar'),
      join(target, 'android-device/libmpp_android_device.so'), join(target, 'aarch64-linux-android/debug/examples/codec_probe')]);
    assert.equal(readFileSync(join(target, 'android-device/libmpp_android_device.so'), 'utf8'), 'native-library');
    const calls = f.calls();
    const cargo = calls.find(call => call.tool === 'cargo');
    assert(cargo.args.includes('--locked'));assert.equal(cargo.autoInstall, '0');
    assert(!cargo.args.includes('--release'));
    assert.equal(cargo.args[cargo.args.indexOf('--target-dir') + 1], target);
    assert(cargo.androidLinker.endsWith('aarch64-linux-android29-clang'));
    assert(!existsSync(cargo.linker));
    const linker = calls.find(call => call.tool === 'clang' && call.args.includes('-shared'));
    assert.deepEqual(linker.args.slice(-2), ['-Wl,-z,max-page-size=16384', '-Wl,-z,common-page-size=16384']);
    assert(linker.args.includes('-Wl,-z,max-page-size=4096'));
    const readelf = calls.find(call => call.tool === 'readelf');
    assert.deepEqual(readelf.args, ['-W', '-l', join(target, 'aarch64-linux-android/debug/libmpp_android_device.so')]);
    const javac = calls.find(call => call.tool === 'javac' && call.args.includes('--release'));
    assert.deepEqual(javac.args.slice(0, 4), ['--release', '8', '-encoding', 'UTF-8']);
    assert.equal(javac.args[javac.args.indexOf('-classpath') + 1], join(f.sdk, 'platforms/android-29/android.jar'));
    assert.equal(javac.args.at(-1), join(f.root, 'android-bootstrap/dev/mpp/Bootstrap.java'));
    const dex = calls.find(call => call.tool === 'd8-36.0.0' && call.args.includes('--min-api'));
    assert.equal(dex.args[dex.args.indexOf('--min-api') + 1], '29');
    assert.equal(dex.args.filter(arg => arg.endsWith('.class')).length, 2);
    assert(!calls.some(call => call.tool === 'd8-37.0.0-rc1'));
    assert(!readdirSync(target).some(name => name.startsWith('.mpp-android-build.')));
  } finally { f.close(); }
});

for (const profile of ['debug', 'release']) {
  test(`builds and packages the ${profile} library and retains the codec probe`, () => {
    const f = fixture();
    try {
      const result = f.run({ MPP_BUILD_PROFILE: profile });
      assert.equal(result.status, 0, result.stderr);
      const target = f.env.CARGO_TARGET_DIR;
      assert.deepEqual(result.stdout.trim().split('\n'), [join(target, 'android-device/bootstrap.jar'),
        join(target, 'android-device/libmpp_android_device.so'), join(target, `aarch64-linux-android/${profile}/examples/codec_probe`)]);
      assert.equal(readFileSync(join(target, 'android-device/libmpp_android_device.so'), 'utf8'), 'native-library');
      const cargo = f.calls().find(call => call.tool === 'cargo');
      assert.equal(cargo.args.includes('--release'), profile === 'release');
      assert(cargo.args.includes('--lib'));
      assert.equal(cargo.args[cargo.args.indexOf('--example') + 1], 'codec_probe');
      assert.equal(cargo.autoInstall, '0');
    } finally { f.close(); }
  });
}

test('rejects an unsupported build profile before probing any tool', () => {
  const f = fixture();
  try {
    const result = f.run({ MPP_BUILD_PROFILE: 'production', CARGO: '/nonexistent-mpp-cargo' });
    assert.equal(result.status, 2);
    assert.match(result.stderr, /MPP_BUILD_PROFILE must be debug or release/);
    assert.deepEqual(f.calls(), []);
    assert(!existsSync(f.env.CARGO_TARGET_DIR));
  } finally { f.close(); }
});

test('honors explicit platform/build-tools and repository-relative Cargo target directory', () => {
  const f = fixture();
  try {
    const result = f.run({ CARGO_TARGET_DIR: 'relative output',
      MPP_BUILD_TOOLS: join(f.sdk, 'build-tools/33.0.1'),
      MPP_ANDROID_JAR: join(f.sdk, 'platforms/android-34/android.jar') });
    assert.equal(result.status, 0, result.stderr);
    assert(existsSync(join(f.root, 'relative output/android-device/bootstrap.jar')));
    const dex = f.calls().find(call => call.tool === 'd8-33.0.1' && call.args.includes('--min-api'));
    assert.equal(dex.args[dex.args.indexOf('--lib') + 1], join(f.sdk, 'platforms/android-34/android.jar'));
  } finally { f.close(); }
});

test('falls back to a newer installed platform while preserving minimum API 29', () => {
  const f = fixture();
  try {
    for (const api of ['29', '30', '32']) rmSync(join(f.sdk, `platforms/android-${api}`), { recursive: true });
    const result = f.run();assert.equal(result.status, 0, result.stderr);
    const javac = f.calls().find(call => call.tool === 'javac' && call.args.includes('-classpath'));
    assert.equal(javac.args[javac.args.indexOf('-classpath') + 1], join(f.sdk, 'platforms/android-34/android.jar'));
    const dex = f.calls().find(call => call.args.includes('--min-api'));
    assert.equal(dex.args[dex.args.indexOf('--min-api') + 1], '29');
  } finally { f.close(); }
});

test('scopes the linker wrapper to Cargo without replacing any caller Rust flag source', () => {
  const f = fixture();
  try {
    const flags = { RUSTFLAGS: '--cfg mpp_caller',
      CARGO_ENCODED_RUSTFLAGS: ['--cfg', 'mpp_encoded="value with spaces"'].join('\x1f'),
      CARGO_TARGET_AARCH64_LINUX_ANDROID_RUSTFLAGS: '-C debuginfo=1', MPP_ANDROID_LINKER: '' };
    const result = f.run(flags);
    assert.equal(result.status, 0, result.stderr);
    for (const call of f.calls()) {
      assert.equal(call.rustflags, flags.RUSTFLAGS);
      assert.equal(call.encodedRustflags, flags.CARGO_ENCODED_RUSTFLAGS);
      assert.equal(call.targetRustflags, flags.CARGO_TARGET_AARCH64_LINUX_ANDROID_RUSTFLAGS);
      if (['rustc', 'javac', 'readelf', 'jar'].includes(call.tool)) assert.equal(call.androidLinker, '');
    }
  } finally { f.close(); }
});

test('accepts ELF load alignment larger than 16 KiB', () => {
  const f = fixture();
  try {
    const result = f.run({ MPP_TEST_ELF_HEADERS: '  LOAD 0x0 0x0 0x0 0x100 0x100 R E 0x10000\n' });
    assert.equal(result.status, 0, result.stderr);
  } finally { f.close(); }
});

for (const [name, extra, message] of [
  ['a misaligned later load segment', { MPP_TEST_ELF_HEADERS:
    '  LOAD 0x0 0x0 0x0 0x100 0x100 R 0x4000\n  LOAD 0x1000 0x1000 0x1000 0x100 0x100 R E 0x1000' }, /below 16 KiB/],
  ['a malformed load alignment', { MPP_TEST_ELF_HEADERS: '  LOAD 0x0 malformed' }, /Invalid ELF load alignment/],
  ['missing load segments', { MPP_TEST_ELF_HEADERS: 'Program Headers:\n  Type Offset VirtAddr PhysAddr FileSiz MemSiz Flg Align' }, /no ELF LOAD segments/],
  ['a failed ELF inspection', { MPP_TEST_READELF_FAIL: '1' }, /Cannot inspect/],
]) {
  test(`rejects ${name} before compiling or replacing previously complete assets`, () => {
    const f = fixture();
    try {
      const assets = join(f.env.CARGO_TARGET_DIR, 'android-device');mkdirSync(assets, { recursive: true });
      for (const file of ['bootstrap.jar', 'libmpp_android_device.so']) writeFileSync(join(assets, file), 'previous');
      const result = f.run(extra);
      assert.equal(result.status, 2, result.stderr);assert.match(result.stderr, message);
      assert(!f.calls().some(call => call.tool === 'javac' && call.args.includes('--release')));
      for (const file of ['bootstrap.jar', 'libmpp_android_device.so']) assert.equal(readFileSync(join(assets, file), 'utf8'), 'previous');
      assert(!readdirSync(f.env.CARGO_TARGET_DIR).some(name => name.startsWith('.mpp-android-build.')));
    } finally { f.close(); }
  });
}

test('rejects a missing NDK ELF inspector before invoking Cargo', () => {
  const f = fixture();
  try {
    rmSync(join(f.ndkBin, 'llvm-readelf'));
    const result = f.run();assert.equal(result.status, 2);assert.match(result.stderr, /NDK ELF inspector/);
    assert(!f.calls().some(call => call.tool === 'cargo'));
  } finally { f.close(); }
});

for (const [name, extra, message] of [
  ['missing JDK', { JAVA_HOME: '/nonexistent-mpp-jdk' }, /JDK 17/],
  ['old JDK', { MPP_TEST_JAVAC_VERSION: '11.0.25' }, /JDK 17/],
  ['missing D8', { MPP_BUILD_TOOLS: '/nonexistent-mpp-build-tools' }, /build-tools with d8/],
  ['missing platform', { MPP_ANDROID_JAR: '/nonexistent-mpp-platform.jar' }, /android.jar/],
]) {
  test(`rejects ${name} before invoking Cargo`, () => {
    const f = fixture();
    try {
      const result = f.run(extra);assert.equal(result.status, 2);assert.match(result.stderr, message);
      assert(!f.calls().some(call => call.tool === 'cargo'));
    } finally { f.close(); }
  });
}

test('failed dex packaging preserves previously complete assets and cleans staging', () => {
  const f = fixture();
  try {
    const assets = join(f.env.CARGO_TARGET_DIR, 'android-device');mkdirSync(assets, { recursive: true });
    for (const file of ['bootstrap.jar', 'libmpp_android_device.so']) writeFileSync(join(assets, file), 'previous');
    const result = f.run({ MPP_TEST_BAD_DEX: '1' });assert.equal(result.status, 2);assert.match(result.stderr, /classes.dex/);
    for (const file of ['bootstrap.jar', 'libmpp_android_device.so']) assert.equal(readFileSync(join(assets, file), 'utf8'), 'previous');
    assert(!readdirSync(f.env.CARGO_TARGET_DIR).some(name => name.startsWith('.mpp-android-build.')));
  } finally { f.close(); }
});

test('Cargo failure never invokes javac compilation or writes packaged assets', () => {
  const f = fixture();
  try {
    assert.equal(f.run({ MPP_TEST_CARGO_FAIL: '1' }).status, 7);
    assert(!f.calls().some(call => call.tool === 'javac' && call.args.includes('--release')));
    assert(!existsSync(join(f.env.CARGO_TARGET_DIR, 'android-device')));
  } finally { f.close(); }
});
