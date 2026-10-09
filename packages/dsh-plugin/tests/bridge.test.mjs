import assert from 'node:assert/strict';
import { once } from 'node:events';
import { chmod, mkdtemp, readFile, rm, writeFile } from 'node:fs/promises';
import { tmpdir } from 'node:os';
import { join } from 'node:path';
import test from 'node:test';

import {
  BOOT_TIMEOUT_MS, BridgeError, EXPECTED_METHODS, REQUEST_LIMIT, RESPONSE_LIMIT,
  createBridge,
} from '../src/host/bridge.mjs';

async function fixture(t, body = 'reply(request, request.params);', hello = {}, startup = '') {
  const directory = await mkdtemp(join(tmpdir(), 'mpp bridge '));
  t.after(() => rm(directory, { recursive: true, force: true }));
  const executable = join(directory, 'fake mpp.mjs');
  const info = { version: '0.1.0', control_protocol: 'mpp/v1', methods: EXPECTED_METHODS, ...hello };
  await writeFile(executable, `#!${process.execPath}
import { createInterface } from 'node:readline';
const hello = ${JSON.stringify(info)};
function envelope(request, result) { return { schema:'mpp/v1', id:request.id, ok:true, result, error:null }; }
function reply(request, result) { process.stdout.write(JSON.stringify(envelope(request, result)) + '\\n'); }
${startup}
createInterface({ input:process.stdin }).on('line', async (line) => {
  const request = JSON.parse(line);
  if (request.method === 'hello') { reply(request, hello); return; }
  ${body}
});
`);
  await chmod(executable, 0o755);
  return executable;
}

async function connected(t, body, hello, options = {}) {
  const executable = await fixture(t, body, hello);
  const bridge = await createBridge({ executable, ...options });
  t.after(() => bridge.close());
  return bridge;
}

test('handshake verifies protocol and required methods', async (t) => {
  for (const hello of [{ control_protocol: 'mpp/v2' }, { methods: ['hello'] }, { version: '' }]) {
    const executable = await fixture(t, undefined, hello);
    await assert.rejects(createBridge({ executable }), { code: 'PROTOCOL_ERROR' });
  }
  const bridge = await connected(t);
  assert.equal(bridge.hello.control_protocol, 'mpp/v1');
  assert.equal(bridge.closed, false);
});

test('native startup errors without a request ID retain their diagnostic and hint', async (t) => {
  const diagnostic = { code: 'TOOL_NOT_FOUND', message: 'Android SDK tools were not found.', hint: 'Install platform-tools or configure the adb executable.' };
  const response = { schema: 'mpp/v1', id: null, ok: false, result: null, error: diagnostic };
  const executable = await fixture(t, undefined, {}, `process.stdout.write(${JSON.stringify(JSON.stringify(response) + '\n')}, () => process.exit(1));`);
  await assert.rejects(createBridge({ executable }), (error) => {
    assert.ok(error instanceof BridgeError);
    assert.equal(error.code, diagnostic.code);
    assert.equal(error.message, diagnostic.message);
    assert.equal(error.hint, diagnostic.hint);
    return true;
  });
});

test('null IDs are fatal only for strict startup errors, never success or later errors', async (t) => {
  const error = { code: 'TOOL_NOT_FOUND', message: 'Missing SDK.', hint: 'Configure adb.' };
  for (const response of [
    { schema: 'mpp/v1', id: null, ok: true, result: {}, error: null },
    { schema: 'mpp/v1', id: null, ok: false, result: null, error, extra: true },
    { schema: 'mpp/v1', id: null, ok: false, result: null, error: { ...error, extra: true } },
    { schema: 'mpp/v1', id: null, ok: false, result: null, error: { ...error, hint: '' } },
  ]) {
    const executable = await fixture(t, undefined, {}, `process.stdout.write(${JSON.stringify(JSON.stringify(response) + '\n')});`);
    await assert.rejects(createBridge({ executable }), { code: 'PROTOCOL_ERROR' });
  }
  const bridge = await connected(t, `process.stdout.write(JSON.stringify({schema:'mpp/v1', id:null, ok:false, result:null, error:${JSON.stringify(error)}}) + '\\n');`);
  await assert.rejects(bridge.request('devices.list'), { code: 'PROTOCOL_ERROR' });
  assert.equal(bridge.closed, true);
  const helloResponse = { schema: 'mpp/v1', id: '1', ok: true, result: { version: '0.1.0', control_protocol: 'mpp/v1', methods: EXPECTED_METHODS }, error: null };
  const lateFailure = { schema: 'mpp/v1', id: null, ok: false, result: null, error };
  const coalesced = `${JSON.stringify(helloResponse)}\n${JSON.stringify(lateFailure)}\n`;
  const executable = await fixture(t, undefined, {}, `process.stdout.write(${JSON.stringify(coalesced)});`);
  await assert.rejects(createBridge({ executable }), { code: 'PROTOCOL_ERROR' });
});

test('configuration paths are absolute executable files and do not allow arbitrary arguments', async (t) => {
  await assert.rejects(createBridge({ executable: 'mpp' }), { code: 'INVALID_CONFIG' });
  await assert.rejects(createBridge({ executable: '/definitely-missing-mpp' }), { code: 'INVALID_CONFIG' });
  const executable = await fixture(t);
  for (const config of [
    { executable, args: ['anything'] },
    { executable, env: { API_KEY: 'secret' } },
    { executable, adb: 'adb' },
    { executable, emulator: '/missing-emulator' },
    { executable, xcrun: 'xcrun' },
    { executable, xcrun: '/missing-xcrun' },
  ]) await assert.rejects(createBridge(config), { code: 'INVALID_CONFIG' });
  await chmod(executable, 0o644);
  if (process.platform !== 'win32') {
    await assert.rejects(createBridge({ executable }), { code: 'INVALID_CONFIG' });
  }
});

test('tool paths are passed as individual arguments and environment credentials are omitted', async (t) => {
  const variable = 'MPP_BRIDGE_TEST_API_KEY';
  const before = process.env[variable];
  process.env[variable] = 'never-forward-this-secret';
  t.after(() => { if (before === undefined) delete process.env[variable]; else process.env[variable] = before; });
  const executable = await fixture(t, `reply(request, { argv:process.argv.slice(2), leaked:process.env.${variable} ?? null, path:typeof process.env.PATH });`);
  const bridge = await createBridge({ executable, adb: executable, emulator: executable, xcrun: executable });
  t.after(() => bridge.close());
  const result = await bridge.request('devices.list');
  assert.deepEqual(result.argv, ['--adb', executable, '--emulator', executable, '--xcrun', executable, 'serve', '--stdio']);
  assert.equal(result.leaked, null);
  assert.equal(result.path, 'string');
});

test('string request IDs route concurrent and coalesced responses correctly', async (t) => {
  const bridge = await connected(t, `
    if (typeof request.id !== 'string') throw new Error('non-string ID');
    globalThis.queued ??= [];
    globalThis.queued.push(request);
    if (globalThis.queued.length === 2) {
      const [first, second] = globalThis.queued;
      process.stdout.write(JSON.stringify(envelope(second, second.params)) + '\\n' + JSON.stringify(envelope(first, first.params)) + '\\n');
    }
  `);
  const results = await Promise.all([
    bridge.request('devices.list', { label: 'first' }),
    bridge.request('devices.list', { label: 'second' }),
  ]);
  assert.deepEqual(results, [{ label: 'first' }, { label: 'second' }]);
});

test('split UTF-8 response chunks reassemble before decoding', async (t) => {
  const bridge = await connected(t, `
    const bytes = Buffer.from(JSON.stringify(envelope(request, { name:'模拟器' })) + '\\n');
    for (const byte of bytes) process.stdout.write(Buffer.from([byte]));
  `);
  assert.deepEqual(await bridge.request('devices.list'), { name: '模拟器' });
});

test('structured host errors preserve code and hint without closing the bridge', async (t) => {
  const bridge = await connected(t, `
    if (request.params.fail) process.stdout.write(JSON.stringify({ schema:'mpp/v1', id:request.id, ok:false, result:null, error:{code:'BUSY', message:'Device is in use.', hint:'Disconnect its owner.'} }) + '\\n');
    else reply(request, {ok:true});
  `);
  await assert.rejects(bridge.request('session.connect', { fail: true }), (error) => {
    assert.ok(error instanceof BridgeError);
    assert.equal(error.code, 'BUSY');
    assert.equal(error.hint, 'Disconnect its owner.');
    return true;
  });
  assert.equal(bridge.closed, false);
  assert.deepEqual(await bridge.request('devices.list'), { ok: true });
});

test('request limit applies to UTF-8 bytes and invalid JSON values never reach the host', async (t) => {
  const bridge = await connected(t, 'reply(request, { received:true });');
  const params = { text: 'x'.repeat(REQUEST_LIMIT) };
  for (const value of [params, { text: '界'.repeat(REQUEST_LIMIT / 2) }, { x: NaN }, { generation: Number.MAX_SAFE_INTEGER + 1 }, { x: 1n }, { x: undefined }]) {
    await assert.rejects(bridge.request('devices.list', value), { code: 'INVALID_ARGUMENT' });
  }
  const cycle = {};
  cycle.self = cycle;
  await assert.rejects(bridge.request('devices.list', cycle), { code: 'INVALID_ARGUMENT' });
  assert.deepEqual(await bridge.request('devices.list'), { received: true });
});

test('request limit includes envelope and accepts its exact boundary', async (t) => {
  const bridge = await connected(t, 'reply(request, { received:true });');
  // hello uses ID 1, so the first public request uses the same one-byte ID width.
  const overhead = Buffer.byteLength(JSON.stringify({ id: '2', method: 'devices.list', params: { text: '' } }));
  assert.deepEqual(await bridge.request('devices.list', { text: 'a'.repeat(REQUEST_LIMIT - overhead) }), { received: true });
  await assert.rejects(bridge.request('devices.list', { text: 'a'.repeat(REQUEST_LIMIT - overhead + 1) }), { code: 'INVALID_ARGUMENT' });
});

test('invalid stdout, unknown IDs and unsafe generations invalidate all leases', async (t) => {
  const bodies = [
    `process.stdout.write('diagnostic must not be on stdout\\n');`,
    `process.stdout.write(Buffer.from([0xff, 10]));`,
    `process.stdout.write('\\ufeff' + JSON.stringify(envelope(request, {})) + '\\n');`,
    `reply({id:'unrequested'}, {});`,
    `reply({id:2}, {});`,
    `reply(request, { generation:9007199254740992 });`,
    `process.stdout.write(JSON.stringify({schema:'mpp/v1', id:request.id, ok:true, result:{}, error:{code:'BAD'}}) + '\\n');`,
    `process.stdout.write('\\n');`,
  ];
  for (const body of bodies) {
    const bridge = await connected(t, body);
    let invalidated = 0;
    bridge.on('invalidated', () => { invalidated++; });
    await assert.rejects(bridge.request('devices.list'), { code: 'PROTOCOL_ERROR' });
    assert.equal(bridge.closed, true);
    assert.equal(invalidated, 1);
    await assert.rejects(bridge.request('devices.list'), { code: 'CLOSED' });
    await bridge.close();
    assert.equal(invalidated, 1);
  }
});

test('oversized or unterminated output cannot grow the response buffer indefinitely', async (t) => {
  const bridge = await connected(t, `process.stdout.write('x'.repeat(${RESPONSE_LIMIT + 1}));`);
  await assert.rejects(bridge.request('devices.list'), { code: 'PROTOCOL_ERROR' });
  await bridge.close();
});

test('an exact-boundary response is accepted', async (t) => {
  const bridge = await connected(t, `
    const overhead = Buffer.byteLength(JSON.stringify(envelope(request, { text:'' })));
    reply(request, {text:'x'.repeat(${RESPONSE_LIMIT} - overhead)});
  `);
  const result = await bridge.request('devices.list');
  assert.ok(result.text.length > RESPONSE_LIMIT - 100);
});

test('child exit rejects pending requests and discards diagnostic secrets', async (t) => {
  const bridge = await connected(t, `process.stderr.write('SUPER_SECRET_DIAGNOSTIC'.repeat(10000), () => process.exit(7));`);
  const invalidation = once(bridge, 'invalidated');
  await assert.rejects(bridge.request('devices.list'), (error) => {
    assert.ok(['HOST_EXITED', 'PROTOCOL_ERROR'].includes(error.code));
    assert.ok(!JSON.stringify(error).includes('SUPER_SECRET'));
    assert.ok(!error.message.includes('SUPER_SECRET'));
    return true;
  });
  await invalidation;
  await bridge.close();
});

test('timeouts terminate the host, reject every pending request, and invalidate once', async (t) => {
  const bridge = await connected(t, 'setInterval(() => {}, 1000);');
  let invalidated = 0;
  bridge.on('invalidated', () => { invalidated++; });
  const requests = [
    bridge.request('devices.list', {}, { timeoutMs: 40 }),
    bridge.request('session.connect', { owner: 'one', device: 'fake' }, { timeoutMs: 1000 }),
  ];
  const results = await Promise.allSettled(requests);
  assert.ok(results.every((result) => result.status === 'rejected' && result.reason.code === 'TIMEOUT'));
  await bridge.close();
  assert.equal(invalidated, 1);
});

test('close awaits termination, cancels pending timers and is idempotent', async (t) => {
  const bridge = await connected(t, 'setInterval(() => {}, 1000);');
  const pending = assert.rejects(bridge.request('devices.list'), { code: 'CLOSED' });
  await bridge.close();
  await pending;
  await bridge.close();
  assert.equal(bridge.closed, true);
});

test('close forcefully reaps a child that ignores SIGTERM', async (t) => {
  const bridge = await connected(t, `
    process.on('SIGTERM', () => {});
    setInterval(() => {}, 1000);
    reply(request, {pid:process.pid});
  `, undefined, { shutdownTimeoutMs: 100 });
  const { pid } = await bridge.request('devices.list');
  await bridge.close();
  assert.throws(() => process.kill(pid, 0), { code: 'ESRCH' });
});

test('close allows asynchronous resource cleanup beyond one second before reaping the host', async t => {
  const directory = await mkdtemp(join(tmpdir(), 'mpp cleanup '));
  t.after(() => rm(directory, { recursive: true, force: true }));
  const marker = join(directory, 'released');
  const executable = await fixture(t, 'reply(request, {});', {}, `
    import {writeFileSync} from 'node:fs';
    process.on('SIGTERM', () => setTimeout(() => {
      writeFileSync(${JSON.stringify(marker)}, 'released'); process.exit(0);
    }, 1300));
    setInterval(() => {}, 1000);
  `);
  const bridge = await createBridge({ executable, shutdownTimeoutMs: 5000 });
  t.after(() => bridge.close());
  await bridge.close();
  assert.equal(await readFile(marker, 'utf8'), 'released');
});

test('pending work is bounded and close rejects the entire queue', async (t) => {
  const bridge = await connected(t, '');
  const pending = Array.from({ length: 64 }, () => bridge.request('devices.list'));
  const settled = Promise.allSettled(pending);
  await assert.rejects(bridge.request('devices.list'), { code: 'BUSY' });
  await bridge.close();
  assert.ok((await settled).every((result) => result.status === 'rejected' && result.reason.code === 'CLOSED'));
});

test('emulator startup reserves at least the Rust boot deadline', async (t) => {
  const bridge = await connected(t);
  await assert.rejects(bridge.request('emulator.start', { avd: 'test', consent: true }, { timeoutMs: BOOT_TIMEOUT_MS - 1 }), { code: 'INVALID_ARGUMENT' });
  assert.deepEqual(await bridge.request('emulator.start', { avd: 'test', consent: true }), { avd: 'test', consent: true });
  const simulator = { udid: 'DEADBEEF-1234-5678-ABCD-123456789ABC', consent: true };
  await assert.rejects(bridge.request('simulator.start', simulator, { timeoutMs: BOOT_TIMEOUT_MS - 1 }), { code: 'INVALID_ARGUMENT' });
  assert.deepEqual(await bridge.request('simulator.start', simulator), simulator);
  for (const timeoutMs of [0, -1, Infinity, 0.5, 600001]) {
    await assert.rejects(bridge.request('devices.list', {}, { timeoutMs }), { code: 'INVALID_ARGUMENT' });
  }
});
