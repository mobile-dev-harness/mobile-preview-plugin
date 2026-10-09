import { randomBytes } from 'node:crypto';
import { statSync } from 'node:fs';
import { chmod, mkdtemp, realpath, rm } from 'node:fs/promises';
import { createConnection } from 'node:net';
import { isAbsolute, join } from 'node:path';
import { Readable } from 'node:stream';
import { BridgeError } from './bridge.mjs';

const LIMIT = 16_384;
const IO_TIMEOUT = 2_000;
const positive = value => Number.isSafeInteger(value) && value > 0;
const object = value => value !== null && typeof value === 'object' && !Array.isArray(value);
const exact = (value, keys) => object(value) && Object.keys(value).length === keys.length
  && keys.every(key => Object.hasOwn(value, key));
const fail = (code, message) => new BridgeError(code, message, 'Stop the preview and reconnect before retrying.');
const aborted = () => fail('ABORTED', 'The preview request was cancelled.');

function descriptor(value, slot) {
  const g = value?.geometry;
  if (!exact(value, ['stream_id', 'epoch', 'generation', 'geometry', 'video_socket', 'control_socket'])
    || value.stream_id !== slot.id || !positive(value.epoch)
    || value.generation !== slot.lease.generation
    || value.video_socket !== join(slot.dir, 'video.sock')
    || value.control_socket !== join(slot.dir, 'control.sock')
    || !exact(g, ['width', 'height', 'display_width', 'display_height', 'rotation'])
    || !['width', 'height', 'display_width', 'display_height'].every(key => positive(g[key]) && g[key] <= 16_384)
    || g.width > 4096 || g.height > 4096 || !Number.isSafeInteger(g.rotation) || g.rotation < 0 || g.rotation > 3) {
    throw fail('PROTOCOL_ERROR', 'MPP returned an invalid preview descriptor.');
  }
  return value;
}

function connect(socket) {
  return new Promise((resolve, reject) => {
    const finish = error => {
      clearTimeout(timer);
      socket.off('connect', ready); socket.off('error', failed); socket.off('close', closed);
      if (error) { socket.destroy(); reject(error); } else resolve();
    };
    const ready = () => finish();
    const failed = () => finish(fail('CONNECTION_FAILED', 'The private preview socket could not connect.'));
    const closed = () => finish(fail('CLOSED', 'The private preview socket closed during connection.'));
    const timer = setTimeout(() => finish(fail('TIMEOUT', 'The private preview socket connection timed out.')), IO_TIMEOUT);
    socket.once('connect', ready); socket.once('error', failed); socket.once('close', closed);
  });
}

/** Exactly one outstanding request; the deadline covers its write and complete reply. */
class ControlChannel {
  #socket; #pending; #failed; #bytes = Buffer.alloc(0); #onFailure;
  constructor(socket, onFailure) {
    this.#socket = socket; this.#onFailure = onFailure;
    socket.on('data', chunk => this.#receive(chunk));
    socket.on('error', () => this.#fail(fail('CONNECTION_FAILED', 'The preview control connection failed.')));
    socket.on('close', () => this.#fail(fail('CLOSED', 'The preview control connection closed.')));
  }
  request(request, line) {
    if (this.#failed) return Promise.reject(this.#failed);
    return new Promise((resolve, reject) => {
      const timer = setTimeout(() => this.#fail(fail('TIMEOUT', 'The complete input reply did not arrive in time.')), IO_TIMEOUT);
      this.#pending = { seq: request.seq, resolve, reject, timer };
      this.#socket.write(line, error => {
        if (error) this.#fail(fail('CONNECTION_FAILED', 'Input could not be delivered.'));
      });
    });
  }
  #fail(error) {
    if (this.#failed) return;
    this.#failed = error;
    if (this.#pending) {
      clearTimeout(this.#pending.timer); this.#pending.reject(error); this.#pending = undefined;
    }
    this.#bytes = Buffer.alloc(0);
    this.#socket.destroy();
    this.#onFailure();
  }
  #receive(chunk) {
    if (this.#failed || chunk.length === 0) return;
    const newline = chunk.indexOf(10);
    const length = newline < 0 ? chunk.length : newline;
    if (!this.#pending || this.#bytes.length + length > LIMIT || (newline >= 0 && newline + 1 !== chunk.length)) {
      this.#fail(fail('PROTOCOL_ERROR', 'Unexpected or oversized input reply.')); return;
    }
    this.#bytes = Buffer.concat([this.#bytes, chunk.subarray(0, length)]);
    if (newline < 0) return;
    let reply;
    try {
      reply = JSON.parse(new TextDecoder('utf-8', { fatal: true }).decode(this.#bytes));
      if (!exact(reply, ['seq', 'ok', 'code', 'message']) || !positive(reply.seq)
        || reply.seq !== this.#pending.seq || typeof reply.ok !== 'boolean'
        || (reply.ok ? reply.code !== null || reply.message !== null
          : typeof reply.code !== 'string' || !reply.code || typeof reply.message !== 'string' || !reply.message)) throw new Error();
    } catch { this.#fail(fail('PROTOCOL_ERROR', 'MPP returned an invalid input reply.')); return; }
    this.#bytes = Buffer.alloc(0);
    const pending = this.#pending; this.#pending = undefined;
    clearTimeout(pending.timer); pending.resolve(reply);
  }
}

function batch(requests, slot) {
  if (!Array.isArray(requests) || requests.length < 1 || requests.length > 64) {
    throw fail('INVALID_ARGUMENT', 'Input requires between 1 and 64 requests.');
  }
  let last = slot.seq;
  return requests.map(request => {
    const command = request?.command;
    if (!exact(request, ['seq', 'epoch', 'command']) || !positive(request.seq)
      || request.seq <= last || request.epoch !== slot.descriptor.epoch
      || !object(command) || !['input', 'reset', 'heartbeat', 'stop', 'key_frame'].includes(command.kind)
      || !exact(command, command.kind === 'input' ? ['kind', 'event'] : ['kind'])) {
      throw fail('INVALID_ARGUMENT', 'Input sequence, capture epoch or command is invalid.');
    }
    if (command.kind === 'input' && !slot.capabilities.input) {
      throw fail('UNSUPPORTED', 'This preview is read-only; device input is unavailable.');
    }
    let line;
    try {
      line = JSON.stringify(request, (_key, value) => {
        if (typeof value === 'number' && !Number.isFinite(value)) throw new Error();
        if (['undefined', 'function', 'symbol'].includes(typeof value)) throw new Error();
        return value;
      });
      if (Buffer.byteLength(line) > LIMIT) throw new Error();
    } catch { throw fail('INVALID_ARGUMENT', 'Each input request must be valid JSON within 16 KiB.'); }
    last = request.seq;
    return { request, line: `${line}\n` };
  });
}

/** Private binding objects are supplied by the authenticated DSH service, never by a browser. */
export class PreviewPool {
  #assets; #rpc; #settings; #slots = new Map(); #disposed = false; #disposing;
  constructor({ assetsDir, maxSize = 1280, bitRate = 4_000_000, maxFps = 30, rpc }) {
    if (typeof rpc !== 'function' || !Number.isSafeInteger(maxSize) || maxSize < 256 || maxSize > 2048 || maxSize % 2
      || !Number.isSafeInteger(bitRate) || bitRate < 100_000 || bitRate > 20_000_000
      || !Number.isSafeInteger(maxFps) || maxFps < 1 || maxFps > 60) throw fail('INVALID_CONFIG', 'Invalid preview configuration.');
    this.#assets = assetsDir; this.#rpc = rpc;
    this.#settings = { max_size: maxSize, bit_rate: bitRate, max_fps: maxFps };
  }
  get available() {
    try {
      return typeof this.#assets === 'string' && isAbsolute(this.#assets)
        && ['bootstrap.jar', 'libmpp_android_device.so'].every(name => statSync(join(this.#assets, name)).isFile());
    } catch { return false; }
  }
  async start(binding, { signal } = {}) {
    if (this.#disposed) throw fail('CLOSED', 'The preview pool has closed.');
    if (signal?.aborted) throw aborted();
    if (this.#slots.has(binding)) throw fail('BUSY', 'This binding already has a preview operation.');
    const device = binding?.session?.device;
    const ios = device?.platform === 'ios';
    if (!ios && !this.available) throw fail('UNSUPPORTED', 'Built Android preview assets are unavailable.');
    if (!positive(binding?.session?.generation) || typeof binding.owner !== 'string' || typeof binding.session.id !== 'string') {
      throw fail('STALE_SESSION', 'The device binding is invalid.');
    }
    if (ios && device.capabilities?.video !== true) throw fail('UNSUPPORTED', 'This device does not support video preview.');
    const slot = { binding, stream: randomBytes(32).toString('base64url'), id: randomBytes(16).toString('hex'),
      lease: { owner: binding.owner, session: binding.session.id, generation: binding.session.generation },
      capabilities: { video: true, input: !ios || device.capabilities?.input === true },
      stopped: false, attempted: false, seq: 0, busy: false, mediaUsed: false, listeners: [] };
    this.#slots.set(binding, slot);
    slot.work = this.#initialize(slot);
    const onAbort = () => { void this.#close(slot).catch(() => {}); };
    signal?.addEventListener('abort', onAbort, { once: true });
    try {
      await slot.work;
      if (slot.stopped || signal?.aborted) throw aborted();
      const { epoch, generation, geometry } = slot.descriptor;
      return { stream: slot.stream, epoch, generation, geometry, capabilities: slot.capabilities };
    } catch (error) {
      await this.#close(slot).catch(() => {});
      throw error;
    } finally { signal?.removeEventListener('abort', onAbort); }
  }
  async #initialize(slot) {
    const directory = await mkdtemp('/tmp/mpp-');
    slot.dir = directory;
    await chmod(directory, 0o700);
    slot.dir = await realpath(directory);
    if (slot.stopped) throw aborted();
    slot.attempted = true;
    const result = await this.#rpc(slot.binding, 'preview.start', {
      ...slot.lease,
      ...(slot.binding.session.device?.platform === 'ios' ? {} : {
        bootstrap: join(this.#assets, 'bootstrap.jar'), library: join(this.#assets, 'libmpp_android_device.so'),
      }),
      socket_dir: slot.dir, stream_id: slot.id, token: randomBytes(32).toString('hex'), ...this.#settings,
    }, 45_000);
    if (positive(result?.epoch)) slot.epoch = result.epoch;
    slot.descriptor = descriptor(result, slot);
    if (slot.stopped) throw aborted();
    slot.controlSocket = createConnection({ path: slot.descriptor.control_socket });
    slot.controlSocket.on('error', () => {});
    await connect(slot.controlSocket);
    if (slot.stopped) throw aborted();
    slot.control = new ControlChannel(slot.controlSocket, () => { void this.#close(slot).catch(() => {}); });
  }
  #get(binding, stream) {
    const slot = this.#slots.get(binding);
    if (!slot || slot.stream !== stream || slot.stopped || !slot.control) throw fail('STALE_SESSION', 'The preview no longer belongs to this binding.');
    return slot;
  }
  media(binding, stream, signal) {
    const slot = this.#get(binding, stream);
    if (slot.mediaUsed) throw fail('BUSY', 'This preview already has a video consumer.');
    if (signal?.aborted) { void this.#close(slot).catch(() => {}); throw aborted(); }
    slot.mediaUsed = true;
    const video = createConnection({ path: slot.descriptor.video_socket, highWaterMark: 65_536 });
    slot.video = video;
    const stop = () => { void this.#close(slot).catch(() => {}); };
    video.on('error', stop); video.once('close', stop);
    void connect(video).catch(stop);
    signal?.addEventListener('abort', stop, { once: true });
    slot.listeners.push(() => signal?.removeEventListener('abort', stop));
    return Readable.toWeb(video, { strategy: { highWaterMark: 65_536, size: chunk => chunk.byteLength } });
  }
  async input(binding, stream, requests, signal) {
    const slot = this.#get(binding, stream);
    if (slot.busy) throw fail('BUSY', 'A preview input batch is still in flight.');
    slot.busy = true;
    const onAbort = () => { void this.#close(slot).catch(() => {}); };
    signal?.addEventListener('abort', onAbort, { once: true });
    try {
      if (signal?.aborted) throw aborted();
      const frames = batch(requests, slot);
      const replies = [];
      for (const { request, line } of frames) {
        if (slot.stopped || signal?.aborted) throw aborted();
        slot.seq = request.seq;
        const reply = await slot.control.request(request, line);
        replies.push(reply);
        if (!reply.ok) { await this.#close(slot); break; }
      }
      return { replies };
    } catch (error) {
      await this.#close(slot).catch(() => {});
      throw signal?.aborted ? aborted() : error;
    } finally { slot.busy = false; signal?.removeEventListener('abort', onAbort); }
  }
  async stop(binding, stream) {
    const slot = this.#slots.get(binding);
    if (slot?.stream === stream) await this.#close(slot);
    return { stopped: true };
  }
  async stopBinding(binding) {
    const slot = this.#slots.get(binding);
    if (slot) await this.#close(slot);
  }
  #close(slot) {
    if (slot.closing) return slot.closing;
    slot.stopped = true;
    slot.controlSocket?.destroy(); slot.video?.destroy();
    for (const remove of slot.listeners) remove();
    slot.listeners.length = 0;
    slot.closing = (async () => {
      await slot.work.catch(() => {});
      try {
        if (slot.attempted && slot.epoch) await this.#rpc(slot.binding, 'preview.stop', {
          ...slot.lease, stream_id: slot.id, epoch: slot.epoch,
        }, 30_000);
      } catch (error) {
        if (!['STALE_SESSION', 'NOT_FOUND', 'CLOSED', 'HOST_EXITED'].includes(error?.code)) throw error;
      } finally {
        try { if (slot.dir) await rm(slot.dir, { recursive: true, force: true }); }
        finally { if (this.#slots.get(slot.binding) === slot) this.#slots.delete(slot.binding); }
      }
    })();
    return slot.closing;
  }
  dispose() {
    if (!this.#disposing) {
      this.#disposed = true;
      this.#disposing = Promise.all([...this.#slots.values()].map(slot => this.#close(slot))).then(() => {});
    }
    return this.#disposing;
  }
}
