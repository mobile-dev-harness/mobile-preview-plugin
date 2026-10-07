import assert from 'node:assert/strict';
import { once } from 'node:events';
import { mkdtemp, rm, stat, writeFile } from 'node:fs/promises';
import { createServer } from 'node:net';
import { join } from 'node:path';
import { test as nodeTest } from 'node:test';
import { PreviewPool } from '../src/host/preview.mjs';

const deferred = () => {
  let resolve;
  const promise = new Promise(accept => { resolve = accept; });
  return { promise, resolve };
};
const delay = ms => new Promise(resolve => setTimeout(resolve, ms));
const test = (name, run) => nodeTest(name, { timeout: 10_000 }, run);
const request = (seq, epoch, kind = 'heartbeat') => ({ seq, epoch, command: { kind } });
const reply = seq => ({ seq, ok: true, code: null, message: null });

async function fixture(t, options = {}) {
  const assets = await mkdtemp('/tmp/mpp-preview-test-');
  for (const name of ['bootstrap.jar', 'libmpp_android_device.so']) await writeFile(join(assets, name), 'test asset');
  const calls = [], states = [], gates = [], timers = [];
  let epoch = 0;
  const binding = { token: 'private-binding', owner: 'chat-owner', session: { id: 'session-1', generation: 9 }, bridge: {} };
  const pool = new PreviewPool({ assetsDir: assets, rpc: async (owner, method, params, timeout) => {
    calls.push({ owner, method, params, timeout });
    if (method === 'preview.stop') {
      if (options.stopGate) { gates.push(options.stopGate); await options.stopGate.promise; }
      const state = states.find(item => item.params.stream_id === params.stream_id);
      if (state) await closeState(state);
      return { stopped: true };
    }
    assert.equal(method, 'preview.start');
    const state = { params, sockets: [], servers: [], requests: [], controlReady: deferred(), videoReady: deferred() };
    states.push(state);
    const control = createServer(socket => {
      state.control = socket; state.sockets.push(socket); socket.on('error', () => {});
      state.controlReady.resolve(socket);
      let text = '';
      socket.on('data', chunk => {
        text += chunk.toString();
        while (text.includes('\n')) {
          const index = text.indexOf('\n');
          const input = JSON.parse(text.slice(0, index)); text = text.slice(index + 1);
          state.requests.push(input);
          if (options.respond) options.respond(input, socket, state, timers);
          else socket.write(`${JSON.stringify(reply(input.seq))}\n`);
        }
      });
    });
    const video = createServer(socket => {
      state.video = socket; state.sockets.push(socket); socket.on('error', () => {});
      state.videoReady.resolve(socket);
    });
    state.servers = [control, video];
    for (const [server, file] of [[control, 'control.sock'], [video, 'video.sock']]) {
      server.listen(join(params.socket_dir, file)); await once(server, 'listening');
    }
    if (options.started) options.started.resolve(state);
    if (options.startGate) { gates.push(options.startGate); await options.startGate.promise; }
    const result = { stream_id: params.stream_id, epoch: ++epoch, generation: params.generation,
      geometry: { width: 480, height: 1066, display_width: 1080, display_height: 2400, rotation: 0 },
      video_socket: join(params.socket_dir, 'video.sock'), control_socket: join(params.socket_dir, 'control.sock') };
    return options.descriptor ? options.descriptor(result) : result;
  } });
  async function closeState(state) {
    for (const socket of state.sockets) socket.destroy();
    await Promise.all(state.servers.map(server => new Promise(resolve => server.close(() => resolve()))));
  }
  t.after(async () => {
    for (const gate of gates) gate.resolve();
    for (const timer of timers) clearInterval(timer);
    await pool.dispose().catch(() => {});
    for (const state of states) await closeState(state);
    await rm(assets, { recursive: true, force: true });
  });
  return { pool, binding, calls, states, assets };
}

test('starts with private assets/tokens and immediately connects control; media has one consumer', async t => {
  const f = await fixture(t);
  assert.equal(f.pool.available, true);
  const result = await f.pool.start(f.binding);
  assert.match(result.stream, /^[A-Za-z0-9_-]{43}$/u);
  assert.deepEqual(result.capabilities, { video: true, input: true });
  assert.equal(result.generation, 9); assert.equal(result.epoch, 1);
  const { params, timeout } = f.calls[0];
  assert.equal(timeout, 45_000); assert.equal(params.owner, 'chat-owner');
  assert.equal(params.session, 'session-1'); assert.match(params.stream_id, /^[a-f0-9]{32}$/u);
  assert.match(params.token, /^[a-f0-9]{64}$/u); assert.notEqual(params.token, result.stream);
  assert.equal(params.bootstrap, join(f.assets, 'bootstrap.jar'));
  assert.equal((await stat(params.socket_dir)).mode & 0o777, 0o700);
  assert(Buffer.byteLength(join(params.socket_dir, 'control.sock')) < 104);
  await f.states[0].controlReady.promise;
  await assert.rejects(f.pool.start(f.binding), { code: 'BUSY' });
  const stream = f.pool.media(f.binding, result.stream);
  assert.throws(() => f.pool.media(f.binding, result.stream), { code: 'BUSY' });
  const sender = await f.states[0].videoReady.promise;
  sender.write(Buffer.from([0, 1, 2, 3]));
  const reader = stream.getReader();assert.deepEqual([...((await reader.read()).value)], [0, 1, 2, 3]);
  await reader.cancel();await f.pool.stopBinding(f.binding);
  assert.equal(f.calls.at(-1).method, 'preview.stop');assert.equal(f.calls.at(-1).timeout, 30_000);
  assert.equal(f.calls.at(-1).params.epoch, result.epoch);
  await assert.rejects(stat(params.socket_dir), { code: 'ENOENT' });
});

test('missing assets report unsupported without spawning RPC work', async () => {
  const pool = new PreviewPool({ rpc: () => { assert.fail('RPC must not run'); } });
  assert.equal(pool.available, false);
  await assert.rejects(pool.start({}), { code: 'UNSUPPORTED' });await pool.dispose();
});

test('rejects descriptor identity, unsafe numbers and paths outside the owned directory', async t => {
  for (const [modify, hasSafeEpoch] of [
    [d => ({ ...d, control_socket: '/tmp/untrusted.sock' }), true],
    [d => ({ ...d, epoch: Number.MAX_SAFE_INTEGER + 1 }), false],
    [d => ({ ...d, generation: 10 }), true],
    [d => ({ ...d, stream_id: '0'.repeat(32) }), true],
    [d => ({ ...d, geometry: { ...d.geometry, rotation: 4 } }), true],
  ]) {
    const f = await fixture(t, { descriptor: modify });
    await assert.rejects(f.pool.start(f.binding), { code: 'PROTOCOL_ERROR' });
    assert.equal(f.calls.at(-1).method, hasSafeEpoch ? 'preview.stop' : 'preview.start');
    await assert.rejects(stat(f.calls[0].params.socket_dir), { code: 'ENOENT' });
  }
});

test('late successful startup is stopped after abort and cleaned without leaking a lease', async t => {
  const startGate = deferred(), started = deferred();
  const f = await fixture(t, { startGate, started });
  const controller = new AbortController();
  const starting = f.pool.start(f.binding, { signal: controller.signal });
  const rejection = assert.rejects(starting, { code: 'ABORTED' });
  const state = await started.promise;
  controller.abort();startGate.resolve();await rejection;
  assert.equal(f.calls.filter(call => call.method === 'preview.stop').length, 1);
  assert.equal(f.calls.filter(call => call.method === 'session.disconnect').length, 0);
  await assert.rejects(stat(state.params.socket_dir), { code: 'ENOENT' });
  const next = await f.pool.start(f.binding);assert.equal(next.epoch, 2);
});

test('dispose closes a pending startup once its RPC settles and rejects new starts', async t => {
  const startGate = deferred(), started = deferred();
  const f = await fixture(t, { startGate, started });
  const starting = f.pool.start(f.binding);const rejection = assert.rejects(starting, { code: 'ABORTED' });
  await started.promise;const closing = f.pool.dispose();startGate.resolve();
  await Promise.all([closing, rejection]);await f.pool.dispose();
  await assert.rejects(f.pool.start(f.binding), { code: 'CLOSED' });
  assert.equal(f.calls.filter(call => call.method === 'preview.stop').length, 1);
});

test('old stop tokens and old media aborts cannot close a new capture', async t => {
  const f = await fixture(t);
  const first = await f.pool.start(f.binding);const controller = new AbortController();
  const firstMedia = f.pool.media(f.binding, first.stream, controller.signal);
  const reader = firstMedia.getReader();const reading = reader.read().catch(() => {});
  await f.pool.stop(f.binding, first.stream);await reading;
  const next = await f.pool.start(f.binding);
  controller.abort();await f.pool.stop(f.binding, first.stream);
  assert.deepEqual(await f.pool.input(f.binding, next.stream, [request(1, next.epoch)]), { replies: [reply(1)] });
  assert.equal(f.calls.filter(call => call.method === 'preview.stop').length, 1);
});

test('local descriptors close before a queued Rust cleanup RPC settles', async t => {
  const stopGate = deferred();const f = await fixture(t, { stopGate });
  const result = await f.pool.start(f.binding);const socket = await f.states[0].controlReady.promise;
  const ended = once(socket, 'close');const stopped = f.pool.stop(f.binding, result.stream);
  await ended;assert.equal(socket.destroyed, true);
  assert(f.calls.some(call => call.method === 'preview.stop'));
  stopGate.resolve();await stopped;
});

test('preserves browser sequence ordering and forbids overlapping batches', async t => {
  let pending;
  const f = await fixture(t, { respond: (input, socket) => { pending = () => socket.write(`${JSON.stringify(reply(input.seq))}\n`); } });
  const result = await f.pool.start(f.binding);
  const first = f.pool.input(f.binding, result.stream, [request(2, result.epoch)]);
  await assert.rejects(f.pool.input(f.binding, result.stream, [request(3, result.epoch)]), { code: 'BUSY' });
  while (!pending) await delay(5);
  pending();assert.deepEqual(await first, { replies: [reply(2)] });
  await assert.rejects(f.pool.input(f.binding, result.stream, [request(2, result.epoch)]), { code: 'INVALID_ARGUMENT' });
  assert.equal(f.states[0].requests.length, 1);
});

test('validates the entire bounded batch before sending any event', async t => {
  for (const requests of [
    [request(1, 1), request(2, 99)],
    Array.from({ length: 65 }, (_, index) => request(index + 1, 1)),
    [{ seq: 1, epoch: 1, command: { kind: 'input', event: { kind: 'text', text: 'x'.repeat(16_384) } } }],
  ]) {
    const f = await fixture(t);const result = await f.pool.start(f.binding);
    await assert.rejects(f.pool.input(f.binding, result.stream, requests), { code: 'INVALID_ARGUMENT' });
    assert.equal(f.states[0].requests.length, 0);
  }
});

for (const [name, response] of [
  ['malformed JSON', Buffer.from('{oops}\n')],
  ['wrong acknowledgement sequence', Buffer.from(`${JSON.stringify(reply(2))}\n`)],
  ['unsafe acknowledgement sequence', Buffer.from(`${JSON.stringify(reply(Number.MAX_SAFE_INTEGER + 1))}\n`)],
  ['oversized reply', Buffer.from(`${' '.repeat(16_385)}\n`)],
  ['invalid UTF-8', Buffer.from([0xff, 10])],
  ['extra unsolicited packet', Buffer.from(`${JSON.stringify(reply(1))}\n${JSON.stringify(reply(2))}\n`)],
]) {
  test(`fails closed on ${name}`, async t => {
    const f = await fixture(t, { respond: (_request, socket) => socket.write(response) });
    const result = await f.pool.start(f.binding);
    await assert.rejects(f.pool.input(f.binding, result.stream, [request(1, result.epoch)]), { code: 'PROTOCOL_ERROR' });
    assert.equal(f.calls.at(-1).method, 'preview.stop');assert.equal(f.states[0].requests.length, 1);
  });
}

test('negative acknowledgement stops the preview and does not inject the remaining batch', async t => {
  const rejected = { seq: 1, ok: false, code: 'UNSUPPORTED', message: 'Input unavailable' };
  const f = await fixture(t, { respond: (_request, socket) => socket.write(`${JSON.stringify(rejected)}\n`) });
  const result = await f.pool.start(f.binding);
  assert.deepEqual(await f.pool.input(f.binding, result.stream, [request(1, result.epoch), request(2, result.epoch)]), { replies: [rejected] });
  assert.equal(f.states[0].requests.length, 1);assert.equal(f.calls.at(-1).method, 'preview.stop');
});

test('accepts a complete reply split across multiple socket reads', async t => {
  const f = await fixture(t, { respond: (input, socket, _state, timers) => {
    const bytes = Buffer.from(`${JSON.stringify(reply(input.seq))}\n`);
    socket.write(bytes.subarray(0, 8));
    timers.push(setTimeout(() => socket.write(bytes.subarray(8)), 10));
  } });
  const result = await f.pool.start(f.binding);
  assert.deepEqual(await f.pool.input(f.binding, result.stream, [request(1, result.epoch)]), { replies: [reply(1)] });
});

test('unsolicited bytes on an idle control channel close the exact preview', async t => {
  const f = await fixture(t);const result = await f.pool.start(f.binding);
  const socket = await f.states[0].controlReady.promise;const closed = once(socket, 'close');
  socket.write('unsolicited');await closed;await f.pool.stopBinding(f.binding);
  await assert.rejects(f.pool.input(f.binding, result.stream, [request(1, result.epoch)]), { code: 'STALE_SESSION' });
});

test('abort destroys an in-flight control exchange without replay', async t => {
  const received = deferred();const f = await fixture(t, { respond: () => received.resolve() });
  const result = await f.pool.start(f.binding);const controller = new AbortController();
  const input = f.pool.input(f.binding, result.stream, [request(1, result.epoch)], controller.signal);
  const rejection = assert.rejects(input, { code: 'ABORTED' });await received.promise;
  controller.abort();await rejection;assert.equal(f.states[0].requests.length, 1);
});

test('reply deadline covers the whole message even when bytes keep arriving', async t => {
  const f = await fixture(t, { respond: (_request, socket, _state, timers) => {
    timers.push(setInterval(() => socket.write(' '), 150));
  } });
  const result = await f.pool.start(f.binding);const began = performance.now();
  await assert.rejects(f.pool.input(f.binding, result.stream, [request(1, result.epoch)]), { code: 'TIMEOUT' });
  assert(performance.now() - began >= 1800);assert.equal(f.calls.at(-1).method, 'preview.stop');
});

test('video backpressure stalls a slow consumer and preserves all bytes after reading resumes', async t => {
  const f = await fixture(t);const result = await f.pool.start(f.binding);
  const media = f.pool.media(f.binding, result.stream);
  const socket = await f.states[0].videoReady.promise;
  const chunk = Buffer.alloc(65_536, 17), total = 64 * 1024 * 1024;
  let sent = 0;
  function write() {
    while (sent < total && !socket.destroyed) {
      sent += chunk.length;
      if (!socket.write(chunk)) { socket.once('drain', write); return; }
    }
  }
  write();await delay(100);assert(sent < total, 'producer must stall while Web stream is unread');
  const reader = media.getReader();let received = 0;
  while (received < total) {
    const { value, done } = await reader.read();assert.equal(done, false);
    assert.equal(value[0], 17);assert.equal(value.at(-1), 17);received += value.byteLength;
  }
  assert.equal(received, total);await reader.cancel();await f.pool.stopBinding(f.binding);
});
