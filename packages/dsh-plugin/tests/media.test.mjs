import assert from 'node:assert/strict';
import { readFile } from 'node:fs/promises';
import test from 'node:test';
import vm from 'node:vm';

const artifact = (await readFile(new URL('../client.js', import.meta.url), 'utf8'))
  .replace("return { inject: ['slots'", "return { createMediaParser, createVideoPlayer, createInputQueue, bindCanvasInput, inject: ['slots'");
const flush = async () => { for (let i = 0; i < 8; i++) await new Promise(resolve => setImmediate(resolve)); };
const deferred = () => { let resolve; const promise = new Promise(done => { resolve = done; }); return { promise, resolve }; };
const geometry = { width: 100, height: 200, display_width: 100, display_height: 200, rotation: 0 };
// Only protocol/decoder-adapter fixtures, not evidence that a browser decoded real H.264.
const csd = Uint8Array.of(0, 0, 0, 1, 0x67, 0x42, 0xc0, 0x1e, 0, 0, 1, 0x68, 0xaa);
const idr = Uint8Array.of(0, 0, 0, 1, 0x65, 0xbb);
const delta = Uint8Array.of(0, 0, 1, 0x41, 0xcc);
const configuration = () => { const bytes = new Uint8Array(8 + csd.length); const v = new DataView(bytes.buffer); v.setUint32(0, 100); v.setUint32(4, 200); bytes.set(csd, 8); return bytes; };
const packet = (kind, bytes, generation = 0x1_0000_0001, timestamp = 0) => {
  const result = new Uint8Array(28 + bytes.length), view = new DataView(result.buffer);
  view.setUint32(0, 0x4d505031); result[4] = kind;
  view.setBigUint64(8, BigInt(generation)); view.setUint32(16, bytes.length); view.setBigUint64(20, BigInt(timestamp));
  result.set(bytes, 28); return result;
};
const concat = (...items) => { const bytes = new Uint8Array(items.reduce((n, item) => n + item.length, 0)); let at = 0; for (const item of items) { bytes.set(item, at); at += item.length; } return bytes; };

function target() {
  const listeners = new Map();
  return { listeners, addEventListener(name, fn) { if (!listeners.has(name)) listeners.set(name, new Set()); listeners.get(name).add(fn); },
    removeEventListener(name, fn) { listeners.get(name)?.delete(fn); },
    dispatch(name, fields = {}) { const event = { preventDefault() {}, ...fields }; for (const fn of listeners.get(name) || []) fn(event); } };
}

function harness({ queueGrowth = false } = {}) {
  let api, next = 0, support = async config => ({ supported: true, config });
  const decoders = [], rafs = new Map(), intervals = new Map(), timers = new Map(), probes = [];
  class VideoDecoder {
    static async isConfigSupported(config) { probes.push(config); return support(config); }
    constructor(callbacks) { Object.assign(this, target()); this.callbacks = callbacks; this.chunks = []; this.decodeQueueSize = 0; this.maxQueue = 0; this.state = 'unconfigured'; decoders.push(this); }
    configure(config) { this.config = config; this.state = 'configured'; }
    decode(chunk) { this.chunks.push(chunk); if (queueGrowth) this.decodeQueueSize++; this.maxQueue = Math.max(this.maxQueue, this.decodeQueueSize); }
    close() { this.state = 'closed'; }
  }
  const window = { ...target(), __ModuleLoader__: { load(def) { api = def.factory(() => ({})); } } };
  const sandbox = { window, Uint8Array, DataView, BigInt, Number, console, queueMicrotask, VideoDecoder,
    EncodedVideoChunk: class { constructor(value) { Object.assign(this, value); this.data = value.data.slice(); } },
    requestAnimationFrame: fn => { const id = ++next; rafs.set(id, fn); return id; }, cancelAnimationFrame: id => rafs.delete(id),
    setInterval: (fn, ms) => { const id = ++next; intervals.set(id, { fn, ms }); return id; }, clearInterval: id => intervals.delete(id),
    setTimeout: (fn, ms) => { const id = ++next; timers.set(id, { fn, ms }); return id; }, clearTimeout: id => timers.delete(id),
  };
  vm.runInNewContext(artifact, sandbox, { filename: 'client.js' });
  const drawn = [], context = { clearRect() { drawn.length = 0; }, drawImage(frame) { drawn.push(frame); } };
  const canvas = { ...target(), getContext: () => context, getBoundingClientRect: () => ({ left: 0, top: 0, width: 300, height: 300 }),
    focus() {}, captured: null, setPointerCapture(id) { this.captured = id; }, hasPointerCapture(id) { return this.captured === id; }, releasePointerCapture() { this.captured = null; } };
  return { api, canvas, window, drawn, decoders, rafs, intervals, timers, probes,
    support: fn => { support = fn; },
    render() { const work = [...rafs.values()]; rafs.clear(); for (const fn of work) fn(); },
  };
}

test('MPP1 parser handles byte chunks, coalescing, u64 generation and independent timestamps', async () => {
  const { api } = harness(), records = [];
  const parser = api.createMediaParser(0x1_0000_0001);
  const bytes = concat(packet(0, configuration()), packet(1, idr, 0x1_0000_0001, 123456), packet(2, delta));
  for (const value of bytes) await parser.push(Uint8Array.of(value), record => records.push(record));
  parser.finish();
  assert.deepEqual(records.map(item => item.kind), [0, 1, 2]);
  assert.equal(records[1].timestamp, 123456);
  assert.deepEqual(records[1].bytes, idr);
  const coalesced = api.createMediaParser(0x1_0000_0001), output = [];
  await coalesced.push(bytes, value => output.push(value)); coalesced.finish();
  assert.equal(output.length, 3);
});

test('MPP1 rejects bad headers, generations, oversize and unsafe PTS before payload allocation', async () => {
  const { api } = harness();
  const variants = [];
  for (const [offset, value] of [[0, 0], [3, 2], [4, 3], [5, 1], [6, 1], [7, 1]]) {
    const bytes = packet(1, idr); bytes[offset] = value; variants.push(bytes);
  }
  for (const length of [0, 8 * 1024 * 1024 + 1, 0xffffffff]) {
    const bytes = packet(1, idr); new DataView(bytes.buffer).setUint32(16, length); variants.push(bytes.subarray(0, 28));
  }
  variants.push(packet(1, idr, 1));
  variants.push(packet(1, idr, 0x1_0000_0001, BigInt(Number.MAX_SAFE_INTEGER) + 1n));
  for (const bytes of variants) {
    const parser = api.createMediaParser(0x1_0000_0001);
    await assert.rejects(parser.push(bytes, () => {}), { key: 'mediaInvalid' });
    assert.throws(() => parser.finish(), { key: 'mediaInvalid' });
  }
  assert.throws(() => api.createMediaParser(Number.MAX_SAFE_INTEGER + 1), { key: 'mediaInvalid' });
});

test('MPP1 rejects truncated buffers and does not accumulate packets behind an async consumer', async () => {
  const { api } = harness(); const bytes = packet(1, idr);
  for (let end = 1; end < bytes.length; end++) {
    const parser = api.createMediaParser(0x1_0000_0001);
    await parser.push(bytes.subarray(0, end), () => {});
    assert.throws(() => parser.finish(), { key: 'mediaInvalid' });
  }
  const wait = deferred(), parser = api.createMediaParser(0x1_0000_0001); let count = 0;
  const work = parser.push(concat(bytes, bytes), async () => { count++; if (count === 1) await wait.promise; });
  await flush(); assert.equal(count, 1); wait.resolve(); await work; assert.equal(count, 2);
});

test('player probes the actual SPS codec and gates deltas until SPS/PPS plus IDR are available', async () => {
  const h = harness(), states = [];
  const player = h.api.createVideoPlayer(h.canvas, geometry, { state: value => states.push(value), keyFrame() {}, failed: assert.fail });
  await player.consume({ kind: 0, bytes: configuration(), timestamp: 0 });
  assert.equal(h.probes[0].codec, 'avc1.42c01e');
  await player.consume({ kind: 2, bytes: delta, timestamp: 1 });
  assert.equal(h.decoders[0].chunks.length, 0);
  await player.consume({ kind: 1, bytes: idr, timestamp: 2 });
  assert.deepEqual(h.decoders[0].chunks[0].data, concat(csd, idr));
  assert.equal(h.decoders[0].chunks[0].type, 'key');
  assert.ok(!states.includes('live')); player.close();
});

test('unsupported codecs and invalid configuration fail instead of claiming a live preview', async () => {
  const h = harness(); h.support(async () => ({ supported: false }));
  const player = h.api.createVideoPlayer(h.canvas, geometry, { state() {}, keyFrame() {}, failed: assert.fail });
  await assert.rejects(player.consume({ kind: 0, bytes: configuration() }), { key: 'previewUnsupported' });
  assert.equal(h.decoders.length, 0);
  const wrong = configuration(); new DataView(wrong.buffer).setUint32(0, 101);
  await assert.rejects(player.consume({ kind: 0, bytes: wrong }), { key: 'mediaInvalid' });
  player.close();
});

test('player keeps only the latest decoded frame and closes pending and late frames', async () => {
  const h = harness(), states = [];
  const player = h.api.createVideoPlayer(h.canvas, geometry, { state: value => states.push(value), keyFrame() {}, failed: assert.fail });
  await player.consume({ kind: 0, bytes: configuration() });
  const frame = () => ({ closed: 0, close() { this.closed++; } });
  const first = frame(), latest = frame();
  h.decoders[0].callbacks.output(first); h.decoders[0].callbacks.output(latest);
  assert.equal(first.closed, 1); assert.equal(h.rafs.size, 1);
  h.render(); assert.equal(latest.closed, 1); assert.equal(h.drawn[0], latest); assert.ok(states.includes('live'));
  const pending = frame(); h.decoders[0].callbacks.output(pending); player.close();
  assert.equal(pending.closed, 1); assert.equal(h.rafs.size, 0); assert.equal(h.drawn.length, 0);
  const late = frame(); h.decoders[0].callbacks.output(late); assert.equal(late.closed, 1);
});

test('decoder errors still recover through one new IDR and discard a suspended old-epoch packet', async () => {
  const h = harness(); let keyRequests = 0;
  const player = h.api.createVideoPlayer(h.canvas, geometry, { state() {}, keyFrame() { keyRequests++; }, failed: assert.fail });
  await player.consume({ kind: 0, bytes: configuration() });
  await player.consume({ kind: 1, bytes: idr, timestamp: 1 });
  h.decoders[0].decodeQueueSize = 4;
  const suspended = player.consume({ kind: 2, bytes: delta, timestamp: 2 });
  await flush();
  h.decoders[0].callbacks.error(new Error('codec error'));
  await suspended;
  assert.equal(h.decoders[0].state, 'closed'); assert.equal(h.decoders.length, 2); assert.equal(keyRequests, 1);
  assert.equal(h.timers.size, 0);
  const late = { closed: false, close() { this.closed = true; } }; h.decoders[0].callbacks.output(late); assert.equal(late.closed, true);
  await player.consume({ kind: 2, bytes: delta, timestamp: 3 }); assert.equal(h.decoders[1].chunks.length, 0);
  await player.consume({ kind: 1, bytes: idr, timestamp: 4 }); assert.equal(h.decoders[1].chunks[0].type, 'key'); player.close();
});

test('a coalesced packet burst waits for dequeue without resetting the decoder or reentering buffering', async () => {
  const h = harness({ queueGrowth: true }), states = []; let keyRequests = 0;
  const player = h.api.createVideoPlayer(h.canvas, geometry, { state: value => states.push(value), keyFrame() { keyRequests++; }, failed: assert.fail });
  const parser = h.api.createMediaParser(0x1_0000_0001);
  const bytes = concat(packet(0, configuration()), packet(1, idr, 0x1_0000_0001, 1),
    ...Array.from({ length: 7 }, (_, index) => packet(2, delta, 0x1_0000_0001, index + 2)));
  const work = parser.push(bytes, item => player.consume(item));
  await flush();
  const decoder = h.decoders[0];
  assert.equal(decoder.chunks.length, 4); assert.equal(decoder.state, 'configured');
  decoder.callbacks.output({ close() {} }); h.render();
  assert.equal(states.at(-1), 'live');
  decoder.decodeQueueSize = 0; decoder.dispatch('dequeue');
  await work; parser.finish();
  assert.equal(h.decoders.length, 1); assert.equal(keyRequests, 0);
  assert.equal(states.filter(value => value === 'buffering').length, 1);
  assert.deepEqual(decoder.chunks.map(chunk => chunk.timestamp), [1, 2, 3, 4, 5, 6, 7, 8]);
  assert.equal(decoder.maxQueue, 4); assert.equal(h.timers.size, 0); player.close();
});

test('closing a capacity-stalled player settles the gate and ignores later dequeue events', async () => {
  const h = harness();
  const player = h.api.createVideoPlayer(h.canvas, geometry, { state() {}, keyFrame: assert.fail, failed: assert.fail });
  await player.consume({ kind: 0, bytes: configuration() });
  await player.consume({ kind: 1, bytes: idr, timestamp: 1 });
  const decoder = h.decoders[0]; decoder.decodeQueueSize = 4;
  const waiting = player.consume({ kind: 2, bytes: delta, timestamp: 2 }); await flush();
  assert.equal(h.timers.size, 1);
  player.close(); await waiting;
  decoder.decodeQueueSize = 0; decoder.dispatch('dequeue'); await flush();
  assert.equal(decoder.chunks.length, 1); assert.equal(h.timers.size, 0);
  assert.equal(decoder.listeners.get('dequeue').size, 0);
});

test('capacity stalls have a bounded deadline and cannot later submit the rejected frame', async () => {
  const h = harness();
  const player = h.api.createVideoPlayer(h.canvas, geometry, { state() {}, keyFrame: assert.fail, failed: assert.fail });
  await player.consume({ kind: 0, bytes: configuration() });
  await player.consume({ kind: 1, bytes: idr, timestamp: 1 });
  const decoder = h.decoders[0]; decoder.decodeQueueSize = 4;
  const waiting = player.consume({ kind: 2, bytes: delta, timestamp: 2 });
  const rejected = assert.rejects(waiting, { key: 'mediaInvalid' }); await flush();
  const timer = [...h.timers.values()][0]; assert.equal(timer.ms, 2000); timer.fn(); await rejected;
  decoder.decodeQueueSize = 0; decoder.dispatch('dequeue'); await flush();
  assert.equal(decoder.chunks.length, 1); assert.equal(h.timers.size, 0);
  assert.equal(decoder.listeners.get('dequeue').size, 0); player.close();
});

test('a delayed dequeue event after window work does not falsely timeout an already drained decoder', async () => {
  const h = harness();
  const player = h.api.createVideoPlayer(h.canvas, geometry, { state() {}, keyFrame: assert.fail, failed: assert.fail });
  await player.consume({ kind: 0, bytes: configuration() });
  await player.consume({ kind: 1, bytes: idr, timestamp: 1 });
  const decoder = h.decoders[0]; decoder.decodeQueueSize = 4;
  const waiting = player.consume({ kind: 2, bytes: delta, timestamp: 2 }); await flush();
  decoder.decodeQueueSize = 0;
  [...h.timers.values()][0].fn(); await waiting;
  assert.deepEqual(decoder.chunks.map(chunk => chunk.timestamp), [1, 2]);
  assert.equal(h.decoders.length, 1); assert.equal(h.timers.size, 0); player.close();
});

test('new configuration invalidates a capacity gate without decoding its old packet into the replacement', async () => {
  const h = harness();
  const player = h.api.createVideoPlayer(h.canvas, geometry, { state() {}, keyFrame: assert.fail, failed: assert.fail });
  await player.consume({ kind: 0, bytes: configuration() });
  await player.consume({ kind: 1, bytes: idr, timestamp: 1 });
  const old = h.decoders[0]; old.decodeQueueSize = 4;
  const waiting = player.consume({ kind: 2, bytes: delta, timestamp: 2 }); await flush();
  await player.consume({ kind: 0, bytes: configuration() }); await waiting;
  old.decodeQueueSize = 0; old.dispatch('dequeue');
  await player.consume({ kind: 1, bytes: idr, timestamp: 100 });
  assert.deepEqual(h.decoders[1].chunks.map(chunk => chunk.timestamp), [100]);
  assert.equal(old.state, 'closed'); assert.equal(h.timers.size, 0); player.close();
});

test('closing during codec support probing never constructs a late decoder', async () => {
  const h = harness(), wait = deferred(); h.support(() => wait.promise);
  const player = h.api.createVideoPlayer(h.canvas, geometry, { state() {}, keyFrame() {}, failed: assert.fail });
  const work = player.consume({ kind: 0, bytes: configuration() }); player.close(); wait.resolve({ supported: true }); await work;
  assert.equal(h.decoders.length, 0);
});

const touch = (phase, x = 0.5) => ({ kind: 'input', event: { kind: 'touch', phase, x, y: 0.5, width: 100, height: 200 } });
const ack = requests => ({ replies: requests.map(item => ({ seq: item.seq, ok: true })) });

test('one input batch is in flight; adjacent moves coalesce without losing edges or sharing lease generation', async () => {
  const h = harness(), wait = deferred(), batches = []; let active = 0, maximum = 0;
  const input = h.api.createInputQueue(97, async requests => {
    active++; maximum = Math.max(maximum, active); batches.push(requests);
    if (batches.length === 1) await wait.promise;
    active--; return ack(requests);
  }, assert.fail);
  input.enqueue(touch('down')); await flush();
  input.enqueue(touch('move', 0.1)); input.enqueue(touch('move', 0.2)); input.enqueue(touch('up')); input.key(4);
  await flush(); assert.equal(batches.length, 1); wait.resolve(); await flush();
  const all = batches.flat();
  assert.deepEqual(all.map(item => item.command.event?.phase), ['down', 'move', 'up', 'down', 'up']);
  assert.equal(all[1].command.event.x, 0.2); assert.ok(all.every(item => item.epoch === 97));
  assert.deepEqual(all.map(item => item.seq), [1, 2, 3, 4, 5]); assert.equal(maximum, 1);
  await input.close(); assert.equal(batches.at(-1)[0].command.kind, 'reset'); assert.equal(batches.at(-1)[0].seq, 6);
});

test('held inputs send 750ms heartbeats through the same sequence and reset releases them', async () => {
  const h = harness(), sent = [];
  const input = h.api.createInputQueue(1, async requests => { sent.push(...requests); return ack(requests); }, assert.fail);
  input.enqueue(touch('down')); await flush();
  assert.equal([...h.intervals.values()][0].ms, 750);
  for (const timer of h.intervals.values()) timer.fn(); await flush();
  input.enqueue({ kind: 'key_frame' }); input.reset(); await flush();
  assert.deepEqual(sent.map(item => item.command.kind), ['input', 'heartbeat', 'key_frame', 'reset']);
  assert.deepEqual(sent.map(item => item.seq), [1, 2, 3, 4]); assert.equal(h.intervals.size, 0); await input.close();
});

test('uncertain input is never replayed; a best-effort reset precedes terminal failure', async () => {
  const h = harness(), batches = [], failures = [];
  const input = h.api.createInputQueue(1, async requests => {
    batches.push(requests); if (batches.length === 1) throw new Error('network'); return ack(requests);
  }, error => failures.push(error));
  input.enqueue(touch('down')); await flush();
  assert.equal(batches.length, 2); assert.equal(batches[1][0].command.kind, 'reset'); assert.equal(failures.length, 1);
  assert.equal(input.enqueue(touch('down')), false); assert.equal(h.intervals.size, 0); await input.close();
});

test('input queue is bounded and batches never exceed 64 commands', async () => {
  const h = harness(), batches = [], failures = [];
  const input = h.api.createInputQueue(1, async requests => { batches.push(requests); return ack(requests); }, error => failures.push(error));
  for (let i = 0; i < 65; i++) input.key(4);
  await flush(); assert.ok(batches.every(batch => batch.length <= 64)); assert.equal(failures.length, 1);
  assert.equal(batches.at(-1)[0].command.kind, 'reset'); await input.close();
});

test('canvas input maps letterboxing, preserves single-pointer ownership and resets on focus/capture loss', () => {
  const h = harness(), commands = [];
  const input = { enqueue: value => commands.push(value), reset: () => commands.push({ kind: 'reset' }) };
  const detach = h.api.bindCanvasInput(h.canvas, geometry, input, () => true);
  h.canvas.dispatch('pointerdown', { pointerId: 1, button: 0, clientX: 20, clientY: 100 }); assert.equal(commands.length, 0);
  h.canvas.dispatch('pointerdown', { pointerId: 1, button: 0, clientX: 150, clientY: 75 });
  assert.equal(commands[0].event.x, 0.5); assert.equal(commands[0].event.y, 0.25);
  h.canvas.dispatch('pointerdown', { pointerId: 2, button: 0, clientX: 150, clientY: 75 }); assert.equal(commands.length, 1);
  h.canvas.dispatch('lostpointercapture', { pointerId: 1 }); assert.equal(commands.at(-1).kind, 'reset');
  h.canvas.dispatch('keydown', { key: 'Enter', repeat: false }); h.canvas.dispatch('keydown', { key: 'Enter', repeat: true });
  assert.equal(commands.filter(item => item.event?.kind === 'key').length, 1);
  h.window.dispatch('blur'); assert.equal(commands.at(-1).kind, 'reset');
  assert.equal(h.window.listeners.has('keydown'), false);
  detach(); assert.ok([...h.canvas.listeners.values()].every(set => set.size === 0));
});
