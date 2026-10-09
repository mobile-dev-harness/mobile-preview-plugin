import assert from 'node:assert/strict';
import { EventEmitter } from 'node:events';
import { randomBytes } from 'node:crypto';
import { setTimeout as sleep } from 'node:timers/promises';
import test from 'node:test';
import { ConnectionService, ServiceError } from '../src/host/service.mjs';
import { PreviewPool } from '../src/host/preview.mjs';

const error = (code) => new ServiceError(code, code);
const deferred = () => {
  let resolve;
  const promise = new Promise((done) => { resolve = done; });
  return { promise, resolve };
};

class MockBridge extends EventEmitter {
  closed = false;
  hello = { control_protocol: 'mpp/v1' };
  leases = new Map();
  calls = [];
  generation = 0;
  block;
  disconnectError;
  statusError;
  async request(method, params, options) {
    this.calls.push({ method, params, options });
    if (this.closed) throw error('CLOSED');
    await this.block?.(method, params);
    if (this.closed) throw error('CLOSED');
    if (method === 'devices.list') return { devices: [{ id: 'android:a' }] };
    if (method === 'emulator.start') return { id: `android:${params.avd}` };
    if (method === 'simulator.start') return { id: `ios:${params.udid}`, platform: 'ios', serial: params.udid };
    if (method === 'session.connect') {
      if ([...this.leases.values()].some((lease) => lease.owner === params.owner || lease.device.id === params.device)) throw error('BUSY');
      const session = { id: `session-${++this.generation}`, owner: params.owner,
        generation: this.generation, state: 'transport_ready', device: { id: params.device, platform: params.device.startsWith('ios:') ? 'ios' : 'android' } };
      this.leases.set(session.id, session);
      return { ...session };
    }
    const session = this.leases.get(params.session);
    if (!session || session.owner !== params.owner || session.generation !== params.generation) throw error('STALE_SESSION');
    if (method === 'session.status') {
      if (this.statusError) {
        const failure = this.statusError;
        this.statusError = undefined;
        this.leases.delete(session.id);
        throw failure;
      }
      return { ...session };
    }
    if (method === 'session.disconnect') {
      if (this.disconnectError) throw this.disconnectError;
      this.leases.delete(session.id);
      return { ...session, state: 'disconnected' };
    }
    throw error('UNSUPPORTED');
  }
  async close() {
    if (this.closed) return;
    this.closed = true;
    this.leases.clear();
    this.emit('invalidated', error('CLOSED'));
  }
}

function fixture(t, options = {}) {
  const bridges = [];
  const sessions = new Set(['chat-a', 'chat-b', 'chat-c']);
  const service = new ConnectionService({
    createBridge: async () => { const bridge = new MockBridge(); bridges.push(bridge); return bridge; },
    validateSession: async (id) => sessions.has(id),
    requestTimeoutMs: 500, bootTimeoutMs: 1000, heartbeatMs: 10, leaseTtlMs: 2000,
    sweepIntervalMs: 2000, iosAvailable: false, ...options,
  });
  t.after(() => service.dispose());
  const call = (method, params = {}, signal) => service.handle({ method, params }, { signal });
  const open = async () => (await call('client.open')).client;
  const connect = (client, sessionId = 'chat-a', device = 'android:a', signal) => call('session.connect', { client, sessionId, device }, signal);
  return { service, bridges, sessions, call, open, connect };
}

const UDID = 'DEADBEEF-1234-5678-ABCD-123456789ABC';

test('iOS backend selection is available without Android assets and preserves platform isolation', async t => {
  const f = fixture(t, { iosAvailable: true });
  const opened = await f.call('client.open');
  assert.deepEqual(opened.capabilities, { video: true, input: false });
  const { client } = opened;
  const selection = await f.service.requestPlatform('chat-a', 'ios');
  assert.equal(selection.available, true);
  assert.equal(f.bridges.length, 0);
  await assert.rejects(f.connect(client), { code: 'INVALID_ARGUMENT' });
  assert.equal(f.bridges.length, 0);
  const connected = await f.connect(client, 'chat-a', `ios:${UDID}`);
  assert.equal(connected.session.device.platform, 'ios');
  await assert.rejects(f.service.requestPlatform('chat-a', 'android'), { code: 'BUSY' });
  assert.deepEqual(await f.call('session.list', { client, sessionId: 'chat-a' }), connected);
  await f.call('session.disconnect', { client, binding: connected.binding });
  await f.service.requestPlatform('chat-a', 'android');
  await assert.rejects(f.connect(client, 'chat-a', `ios:${UDID}`), { code: 'INVALID_ARGUMENT' });
});

test('device inventory forwards only a validated optional platform filter', async t => {
  const f = fixture(t, { iosAvailable: true });
  const client = await f.open();
  for (const platform of [undefined, 'android', 'ios']) {
    const filter = platform === undefined ? {} : { platform };
    await f.call('devices.list', { client, ...filter });
    assert.deepEqual(f.bridges[0].calls.at(-1).params, filter);
  }
  for (const params of [{ platform: null }, { platform: 'windows' }, { platform: 'ios', udid: UDID }]) {
    await assert.rejects(f.call('devices.list', { client, ...params }), { code: 'INVALID_ARGUMENT' });
  }
  assert.equal(f.bridges[0].calls.length, 3);
});

test('simulator startup requires a bounded UDID and explicit consent and uses the boot deadline', async t => {
  const f = fixture(t, { iosAvailable: true });
  const client = await f.open();
  for (const consent of [false, null, 1, 'true']) {
    await assert.rejects(f.call('simulator.start', { client, udid: UDID, consent }), { code: 'PERMISSION_DENIED' });
  }
  for (const params of [{ udid: UDID }, { udid: UDID, consent: true, avd: 'other' },
    ...['', 'booted', 'ios:' + UDID, 'x'.repeat(257), UDID + '\n'].map(udid => ({ udid, consent: true }))]) {
    await assert.rejects(f.call('simulator.start', { client, ...params }), { code: 'INVALID_ARGUMENT' });
  }
  assert.equal(f.bridges.length, 0);
  const started = await f.call('simulator.start', { client, udid: UDID, consent: true });
  assert.equal(started.id, `ios:${UDID}`);
  assert.deepEqual(f.bridges[0].calls[0], {
    method: 'simulator.start', params: { udid: UDID, consent: true }, options: { timeoutMs: 1000 },
  });
});

test('cancelling a consented simulator boot preserves the host and serialized discovery', async t => {
  const f = fixture(t, { iosAvailable: true });
  const client = await f.open();
  await f.call('devices.list', { client, platform: 'ios' });
  const entered = deferred(), complete = deferred(), controller = new AbortController();
  f.bridges[0].block = async method => {
    if (method === 'simulator.start') { entered.resolve(); await complete.promise; }
  };
  const boot = f.call('simulator.start', { client, udid: UDID, consent: true }, controller.signal);
  const rejected = assert.rejects(boot, { code: 'ABORTED' });
  await entered.promise;
  controller.abort(); await rejected;
  assert.deepEqual(await f.call('client.heartbeat', { client }), { alive: true });
  const inventory = f.call('devices.list', { client, platform: 'ios' });
  await sleep(0);
  assert.deepEqual(f.bridges[0].calls.map(item => item.method), ['devices.list', 'simulator.start']);
  complete.resolve(); await inventory;
  assert.equal(f.bridges[0].closed, false);
  assert.equal(f.bridges[0].leases.size, 0);
});

test('one host owns conflicts across clients and conversations; remount returns opaque binding', async (t) => {
  const f = fixture(t);
  const first = await f.open(); const second = await f.open();
  const connected = await f.connect(first);
  assert.match(connected.binding, /^[A-Za-z0-9_-]{43}$/u);
  assert.notEqual(connected.session.owner, 'chat-a');
  assert.ok(connected.session.owner.length <= 128);
  await assert.rejects(f.connect(second, 'chat-b'), { code: 'BUSY' });
  await assert.rejects(f.connect(second, 'chat-a', 'android:b'), { code: 'BUSY' });
  assert.equal(f.bridges.length, 1);
  assert.deepEqual(await f.call('session.list', { client: first, sessionId: 'chat-a' }), connected);
  assert.equal(await f.call('session.list', { client: second, sessionId: 'chat-a' }), null);
  assert.deepEqual(await f.call('session.status', { client: first, binding: connected.binding }), connected);
});

test('preview cleanup failure invalidates the host instead of stranding an invisible lease', async t => {
  const f = fixture(t);
  const client = await f.open();
  const connected = await f.connect(client);
  const original = PreviewPool.prototype.stopBinding;
  try {
    PreviewPool.prototype.stopBinding = async () => { throw error('CLEANUP_FAILED'); };
    await assert.rejects(f.call('session.disconnect', { client, binding: connected.binding }), { code: 'CLEANUP_FAILED' });
    assert.equal(f.bridges[0].closed, true);
    assert.equal(f.bridges[0].leases.size, 0);
  } finally { PreviewPool.prototype.stopBinding = original; }
  const replacement = await f.connect(client);
  assert.equal(f.bridges.length, 2);
  assert.equal(replacement.session.state, 'transport_ready');
});

test('rejects forged fields, foreign bindings and stale handles without trusting browser ownership', async (t) => {
  const f = fixture(t);
  const first = await f.open(); const second = await f.open();
  const { binding } = await f.connect(first);
  await assert.rejects(f.call('session.status', { client: second, binding }), { code: 'STALE_SESSION' });
  await assert.rejects(f.call('session.connect', { client: second, sessionId: 'chat-b', device: 'android:b', owner: 'chat-a' }), { code: 'INVALID_ARGUMENT' });
  await assert.rejects(f.call('session.status', { client: first, binding: randomBytes(32).toString('base64url') }), { code: 'STALE_SESSION' });
  await assert.rejects(f.call('session.status', { client: first, binding: 'session-1' }), { code: 'INVALID_ARGUMENT' });
  const receipt = await f.call('session.disconnect', { client: first, binding });
  assert.equal(receipt.session.state, 'disconnected');
  await assert.rejects(f.call('session.status', { client: first, binding }), { code: 'STALE_SESSION' });
  assert.equal(f.bridges[0].leases.size, 0);
});

test('closing a client cleans only its established leases', async (t) => {
  const f = fixture(t);
  const first = await f.open(); const second = await f.open();
  await f.connect(first);
  const other = await f.connect(second, 'chat-b', 'android:b');
  assert.deepEqual(await f.call('client.close', { client: first }), { closed: true });
  assert.equal(f.bridges[0].leases.size, 1);
  assert.deepEqual(await f.call('session.status', { client: second, binding: other.binding }), other);
  await assert.rejects(f.call('devices.list', { client: first }), { code: 'CLIENT_EXPIRED' });
});

test('host invalidation drops bindings and a retry creates one replacement host', async (t) => {
  const f = fixture(t);
  const client = await f.open(); const prior = await f.connect(client);
  await f.bridges[0].close();
  assert.equal(await f.call('session.list', { client, sessionId: 'chat-a' }), null);
  await assert.rejects(f.call('session.status', { client, binding: prior.binding }), { code: 'STALE_SESSION' });
  const next = await f.connect(client);
  assert.notEqual(next.binding, prior.binding);
  assert.equal(f.bridges.length, 2);
});

test('an unplug probe failure forgets its binding so remount finds the new connection', async (t) => {
  const f = fixture(t);
  const client = await f.open(); const prior = await f.connect(client);
  f.bridges[0].statusError = error('NOT_FOUND');
  await assert.rejects(f.call('session.status', { client, binding: prior.binding }), { code: 'NOT_FOUND' });
  assert.equal(f.bridges[0].leases.size, 0);
  const current = await f.connect(client);
  assert.notEqual(current.binding, prior.binding);
  assert.deepEqual(await f.call('session.list', { client, sessionId: 'chat-a' }), current);
  await assert.rejects(f.call('session.status', { client, binding: prior.binding }), { code: 'STALE_SESSION' });
});

for (const method of ['session.status', 'session.list', 'client.heartbeat']) {
  test(`${method} releases a binding after its DSH conversation is deleted`, async (t) => {
    const f = fixture(t);
    const client = await f.open(); const { binding } = await f.connect(client);
    f.sessions.delete('chat-a');
    const params = { client, ...(method === 'session.status' ? { binding } : method === 'session.list' ? { sessionId: 'chat-a' } : {}) };
    if (method === 'client.heartbeat') await f.call(method, params);
    else await assert.rejects(f.call(method, params), { code: 'NOT_FOUND' });
    assert.equal(f.bridges[0].leases.size, 0);
  });
}

test('validation errors other than missing conversations preserve leases and actionable errors', async (t) => {
  let unavailable = false;
  const f = fixture(t, { validateSession: async () => { if (unavailable) throw error('VALIDATION_UNAVAILABLE'); return true; } });
  const client = await f.open(); await f.connect(client);
  unavailable = true;
  await assert.rejects(f.call('client.heartbeat', { client }), { code: 'VALIDATION_UNAVAILABLE' });
  assert.equal(f.bridges[0].leases.size, 1);
});

test('conversation deleted during connect releases its newly returned lease', async (t) => {
  const f = fixture(t);
  const client = await f.open(); await f.call('devices.list', { client });
  const entered = deferred(); const complete = deferred();
  f.bridges[0].block = async (method) => { if (method === 'session.connect') { entered.resolve(); await complete.promise; } };
  const connecting = f.connect(client);
  await entered.promise; f.sessions.delete('chat-a'); complete.resolve();
  await assert.rejects(connecting, { code: 'NOT_FOUND' });
  assert.equal(f.bridges[0].leases.size, 0);
  assert.ok(f.bridges[0].calls.some(({ method }) => method === 'session.disconnect'));
});

test('failed cleanup tears down the shared host rather than stranding an invisible lease', async (t) => {
  const f = fixture(t);
  const client = await f.open(); await f.connect(client);
  f.bridges[0].disconnectError = error('IO_ERROR');
  await f.call('client.close', { client });
  assert.equal(f.bridges[0].closed, true);
  assert.equal(f.bridges[0].leases.size, 0);
});

test('abort after reservation releases the lease even if conversation validation ignores cancellation', async (t) => {
  const entered = deferred(); const never = new Promise(() => {}); let validations = 0;
  const f = fixture(t, { validateSession: async () => {
    if (++validations === 2) { entered.resolve(); return never; }
    return true;
  } });
  const client = await f.open(); const controller = new AbortController();
  const connecting = f.connect(client, 'chat-a', 'android:a', controller.signal);
  const rejected = assert.rejects(connecting, { code: 'ABORTED' });
  await entered.promise;
  assert.equal(f.bridges[0].leases.size, 1);
  controller.abort(); await rejected; await sleep(0);
  assert.equal(f.bridges[0].leases.size, 0);
  assert.equal(f.bridges[0].closed, false);
});

for (const cause of ['abort', 'timeout', 'expiry']) {
  test(`${cause} during a late connect cannot leave an orphan lease`, async (t) => {
    const f = fixture(t, { requestTimeoutMs: cause === 'timeout' ? 30 : 500,
      leaseTtlMs: cause === 'expiry' ? 30 : 2000, heartbeatMs: 5 });
    const client = await f.open(); await f.call('devices.list', { client });
    const entered = deferred(); const complete = deferred(); const controller = new AbortController();
    f.bridges[0].block = async (method) => { if (method === 'session.connect') { entered.resolve(); await complete.promise; } };
    const connecting = f.connect(client, 'chat-a', 'android:a', controller.signal);
    const rejected = assert.rejects(connecting, { code: { abort: 'ABORTED', timeout: 'TIMEOUT', expiry: 'CLIENT_EXPIRED' }[cause] });
    await entered.promise;
    if (cause === 'abort') controller.abort();
    if (cause === 'expiry') { await sleep(40); await f.service.sweep(); }
    await rejected;
    assert.equal(f.bridges[0].closed, cause === 'timeout');
    complete.resolve(); await sleep(0);
    assert.equal(f.bridges[0].leases.size, 0);
    if (cause !== 'timeout') assert.ok(f.bridges[0].calls.some(({ method }) => method === 'session.disconnect'));
  });
}

test('heartbeats continue while boot is blocked, and a queued timeout never sends a connect', async (t) => {
  const f = fixture(t, { requestTimeoutMs: 35, bootTimeoutMs: 500, leaseTtlMs: 100, heartbeatMs: 10 });
  const client = await f.open(); await f.call('devices.list', { client });
  const entered = deferred(); const complete = deferred();
  f.bridges[0].block = async (method) => { if (method === 'emulator.start') { entered.resolve(); await complete.promise; } };
  const boot = f.call('emulator.start', { client, avd: 'Phone_API_32', consent: true });
  await entered.promise;
  const connecting = f.connect(client);
  const rejected = assert.rejects(connecting, { code: 'TIMEOUT' });
  for (let i = 0; i < 3; i++) { await sleep(15); await f.call('client.heartbeat', { client }); }
  await rejected;
  assert.equal(f.bridges[0].closed, false);
  complete.resolve(); await boot;
  assert.equal(f.bridges[0].calls.filter(({ method }) => method === 'session.connect').length, 0);
  assert.equal(f.bridges[0].leases.size, 0);
});

test('queued cancellation cannot let later inventory requests overtake an active boot', async (t) => {
  const f = fixture(t);
  const client = await f.open(); await f.call('devices.list', { client });
  const entered = deferred(); const complete = deferred();
  f.bridges[0].block = async (method) => { if (method === 'emulator.start') { entered.resolve(); await complete.promise; } };
  const boot = f.call('emulator.start', { client, avd: 'Phone_API_32', consent: true });
  await entered.promise;
  const controller = new AbortController();
  const cancelled = f.connect(client, 'chat-a', 'android:a', controller.signal);
  controller.abort(); await assert.rejects(cancelled, { code: 'ABORTED' });
  const inventory = f.call('devices.list', { client });
  await sleep(0);
  assert.deepEqual(f.bridges[0].calls.map(({ method }) => method), ['devices.list', 'emulator.start']);
  complete.resolve(); await boot; await inventory;
  assert.equal(f.bridges[0].closed, false);
});

test('cancelling status before it is sent preserves the live binding', async (t) => {
  const f = fixture(t);
  const client = await f.open(); const connected = await f.connect(client);
  const entered = deferred(); const complete = deferred();
  f.bridges[0].block = async (method) => { if (method === 'emulator.start') { entered.resolve(); await complete.promise; } };
  const boot = f.call('emulator.start', { client, avd: 'Phone_API_32', consent: true });
  await entered.promise;
  const controller = new AbortController();
  const status = f.call('session.status', { client, binding: connected.binding }, controller.signal);
  controller.abort(); await assert.rejects(status, { code: 'ABORTED' });
  complete.resolve(); await boot;
  assert.deepEqual(await f.call('session.list', { client, sessionId: 'chat-a' }), connected);
  assert.equal(f.bridges[0].leases.size, 1);
});

test('cancelling a consented boot lets it finish, and deletion cleanup never blocks heartbeats', async (t) => {
  const f = fixture(t);
  const client = await f.open(); await f.connect(client);
  const entered = deferred(); const complete = deferred(); const controller = new AbortController();
  f.bridges[0].block = async (method) => { if (method === 'emulator.start') { entered.resolve(); await complete.promise; } };
  const boot = f.call('emulator.start', { client, avd: 'Phone_API_32', consent: true }, controller.signal);
  const rejected = assert.rejects(boot, { code: 'ABORTED' });
  await entered.promise; controller.abort(); await rejected;
  f.sessions.delete('chat-a');
  assert.deepEqual(await Promise.race([
    f.call('client.heartbeat', { client }),
    sleep(100).then(() => { throw new Error('heartbeat was blocked by boot'); }),
  ]), { alive: true });
  assert.equal(f.bridges[0].closed, false);
  assert.equal(f.bridges[0].calls.filter(({ method }) => method === 'session.disconnect').length, 0);
  complete.resolve(); await sleep(0);
  assert.equal(f.bridges[0].leases.size, 0);
});

test('bounds clients and request fields, and requires explicit emulator consent', async (t) => {
  const f = fixture(t, { maxClients: 1 });
  const opened = await f.call('client.open');
  assert.equal(typeof opened.host, 'string');
  assert.equal(opened.requestTimeoutMs, 500);
  assert.equal(opened.bootTimeoutMs, 1000);
  assert.deepEqual(opened.capabilities, { video: false, input: false });
  await assert.rejects(f.open(), { code: 'BUSY' });
  for (const device of ['', 'a\0b', 'a'.repeat(257), ' leading']) {
    await assert.rejects(f.connect(opened.client, 'chat-a', device), { code: 'INVALID_ARGUMENT' });
  }
  await assert.rejects(f.call('emulator.start', { client: opened.client, avd: 'Phone', consent: false }), { code: 'PERMISSION_DENIED' });
  await assert.rejects(f.call('devices.list', { client: opened.client, executable: '/tmp/evil' }), { code: 'INVALID_ARGUMENT' });
  assert.equal(f.bridges.length, 0);
});

test('dispose closes a bridge whose factory resolves late and rejects further clients', async (t) => {
  const complete = deferred(); const entered = deferred(); const bridge = new MockBridge();
  const f = fixture(t, { createBridge: async () => { entered.resolve(); await complete.promise; return bridge; } });
  const client = await f.open();
  const pending = f.connect(client);
  const rejected = assert.rejects(pending, { code: 'CLIENT_EXPIRED' });
  await entered.promise;
  const disposing = f.service.dispose(); complete.resolve();
  await disposing; await rejected;
  assert.equal(bridge.closed, true);
  await assert.rejects(f.open(), { code: 'CLOSED' });
});

test('platform landing is session-scoped and does not require Android discovery', async t => {
  const f = fixture(t);
  const client = await f.open();
  const empty = await f.call('platform.get', { client, sessionId: 'chat-a' });
  assert.equal(empty.platform, null);
  assert.equal(empty.source, null);
  assert.equal(empty.available, false);
  const android = await f.call('platform.select', { client, sessionId: 'chat-a', platform: 'android' });
  assert.equal(android.source, 'user');
  assert.equal(android.available, true);
  const ios = await f.service.requestPlatform('chat-b', 'ios');
  assert.equal(ios.available, false);
  assert.equal(ios.source, 'agent');
  assert.ok(ios.revision > android.revision);
  assert.equal(ios.epoch, android.epoch);
  assert.deepEqual(await f.call('platform.get', { client, sessionId: 'chat-a' }), android);
  assert.deepEqual(await f.call('platform.get', { client, sessionId: 'chat-b' }), ios);
  assert.equal(f.bridges.length, 0);
});

test('each explicit agent request advances revision and manual selection supersedes it', async t => {
  const f = fixture(t);
  const client = await f.open();
  const one = await f.service.requestPlatform('chat-a', 'android');
  const two = await f.service.requestPlatform('chat-a', 'android');
  assert.ok(two.revision > one.revision);
  const manual = await f.call('platform.select', { client, sessionId: 'chat-a', platform: 'ios' });
  assert.ok(manual.revision > two.revision);
  assert.equal(manual.source, 'user');
  two.platform = 'ios';
  assert.deepEqual(await f.call('platform.get', { client, sessionId: 'chat-a' }), manual);
  const nextClient = await f.open();
  await f.call('client.close', { client });
  assert.deepEqual(await f.call('platform.get', { client: nextClient, sessionId: 'chat-a' }), manual);
});

test('platform requests validate platform, browser client, fields and conversation', async t => {
  const f = fixture(t);
  const client = await f.open();
  for (const platform of [null, '', 'Android', 'windows', { platform: 'ios' }]) {
    await assert.rejects(f.call('platform.select', { client, sessionId: 'chat-a', platform }), { code: 'INVALID_ARGUMENT' });
    await assert.rejects(f.service.requestPlatform('chat-a', platform), { code: 'INVALID_ARGUMENT' });
  }
  await assert.rejects(f.call('platform.get', { client: randomBytes(32).toString('base64url'), sessionId: 'chat-a' }), { code: 'CLIENT_EXPIRED' });
  await assert.rejects(f.call('platform.select', { client, sessionId: 'chat-a', platform: 'android', source: 'agent' }), { code: 'INVALID_ARGUMENT' });
  await assert.rejects(f.service.requestPlatform('missing-chat', 'ios'), { code: 'NOT_FOUND' });
  await f.service.requestPlatform('chat-a', 'ios');
  f.sessions.delete('chat-a');
  await assert.rejects(f.call('platform.get', { client, sessionId: 'chat-a' }), { code: 'NOT_FOUND' });
  f.sessions.add('chat-a');
  assert.equal((await f.call('platform.get', { client, sessionId: 'chat-a' })).platform, null);
});

test('cancelled or timed-out platform validation cannot commit a selection', async t => {
  const gate = deferred();
  const f = fixture(t, { validateSession: () => gate.promise, requestTimeoutMs: 30 });
  const client = await f.open();
  const abort = new AbortController();
  const pending = f.service.requestPlatform('chat-a', 'ios', { signal: abort.signal });
  abort.abort();
  await assert.rejects(pending, { code: 'ABORTED' });
  await assert.rejects(f.service.requestPlatform('chat-b', 'android'), { code: 'TIMEOUT' });
  gate.resolve(true);
  assert.equal((await f.call('platform.get', { client, sessionId: 'chat-a' })).platform, null);
  assert.equal((await f.call('platform.get', { client, sessionId: 'chat-b' })).platform, null);
});

test('platform reads run while native work is busy and cannot change a connecting chat', async t => {
  const f = fixture(t);
  const client = await f.open();
  await f.call('devices.list', { client });
  const entered = deferred(), finish = deferred();
  f.bridges[0].block = async method => {
    if (method === 'session.connect') { entered.resolve(); await finish.promise; }
  };
  const connecting = f.connect(client);
  await entered.promise;
  assert.equal((await f.call('platform.get', { client, sessionId: 'chat-a' })).platform, null);
  await assert.rejects(f.service.requestPlatform('chat-a', 'ios'), { code: 'BUSY' });
  assert.equal((await f.service.requestPlatform('chat-b', 'ios')).platform, 'ios');
  finish.resolve();
  await connecting;
});

test('platform switch preserves active connection; iOS intent cannot connect Android', async t => {
  const f = fixture(t);
  const client = await f.open();
  const connected = await f.connect(client);
  const android = await f.service.requestPlatform('chat-a', 'android');
  await assert.rejects(f.service.requestPlatform('chat-a', 'ios'), { code: 'BUSY' });
  assert.deepEqual(await f.call('session.list', { client, sessionId: 'chat-a' }), connected);
  assert.deepEqual(await f.call('platform.get', { client, sessionId: 'chat-a' }), android);
  await f.call('session.disconnect', { client, binding: connected.binding });
  await f.service.requestPlatform('chat-a', 'ios');
  const before = f.bridges[0].calls.length;
  await assert.rejects(f.connect(client), { code: 'UNSUPPORTED' });
  assert.equal(f.bridges[0].calls.length, before);
});

test('platform registry has a bounded working set and uses fresh epochs per host', async t => {
  const f = fixture(t, { validateSession: async () => true });
  const client = await f.open();
  const first = await f.service.requestPlatform('chat-0', 'ios');
  for (let i = 1; i <= 256; i++) await f.service.requestPlatform(`chat-${i}`, 'android');
  const evicted = await f.call('platform.get', { client, sessionId: 'chat-0' });
  assert.equal(evicted.platform, null);
  assert.ok(evicted.revision > first.revision);
  const other = fixture(t);
  assert.notEqual((await other.service.requestPlatform('chat-a', 'ios')).epoch, first.epoch);
  await f.service.dispose();
  await assert.rejects(f.service.requestPlatform('chat-a', 'android'), { code: 'CLOSED' });
});

test('a newer user choice supersedes an older agent validation even when it finishes late', async t => {
  const entered = deferred(), finish = deferred();
  let reads = 0;
  const f = fixture(t, { validateSession: async () => {
    if (++reads === 1) { entered.resolve(); return finish.promise; }
    return true;
  } });
  const client = await f.open();
  const agent = f.service.requestPlatform('chat-a', 'ios');
  const rejected = assert.rejects(agent, { code: 'ABORTED' });
  await entered.promise;
  const user = await f.call('platform.select', { client, sessionId: 'chat-a', platform: 'android' });
  finish.resolve(true);
  await rejected;
  assert.deepEqual(await f.call('platform.get', { client, sessionId: 'chat-a' }), user);
});

test('disposing the service cancels unresolved agent platform requests', async t => {
  const entered = deferred();
  let validationSignal;
  const f = fixture(t, { validateSession: async (_id, signal) => {
    validationSignal = signal; entered.resolve();
    return new Promise(() => {});
  } });
  const pending = f.service.requestPlatform('chat-a', 'android');
  const rejected = assert.rejects(pending, { code: 'CLOSED' });
  await entered.promise;
  await f.service.dispose();
  await rejected;
  assert.equal(validationSignal.aborted, true);
});
