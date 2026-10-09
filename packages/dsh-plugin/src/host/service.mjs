import { createHash, randomBytes } from 'node:crypto';
import { hostname } from 'node:os';
import { PreviewPool } from './preview.mjs';

export class ServiceError extends Error {
  constructor(code, message, hint = 'Reconnect the device from the current conversation and retry.') {
    super(message);
    this.name = 'ServiceError';
    this.code = code;
    this.hint = hint;
  }
}

const fail = (code, message) => new ServiceError(code, message);
const token = () => randomBytes(32).toString('base64url');
const fields = Object.freeze({
  'client.open': [], 'client.heartbeat': ['client'], 'client.close': ['client'],
  'platform.get': ['client', 'sessionId'],
  'platform.select': ['client', 'sessionId', 'platform'],
  'devices.list': ['client', 'platform'], 'emulator.start': ['client', 'avd', 'consent'],
  'simulator.start': ['client', 'udid', 'consent'],
  'session.connect': ['client', 'sessionId', 'device'],
  'session.status': ['client', 'binding'], 'session.disconnect': ['client', 'binding'],
  'session.list': ['client', 'sessionId'],
  'preview.start': ['client', 'binding'],
  'preview.stop': ['client', 'binding', 'stream'],
  'preview.media': ['client', 'binding', 'stream'],
  'input.send': ['client', 'binding', 'stream', 'requests'],
});
const object = (value) => value !== null && typeof value === 'object' && !Array.isArray(value);
const missingSession = (error) => ['NOT_FOUND', 'SESSION_NOT_FOUND'].includes(error?.code);

function boundedString(value, name, limit) {
  if (typeof value !== 'string' || !value.trim() || value !== value.trim()
    || Buffer.byteLength(value) > limit || /[\u0000-\u001f\u007f]/u.test(value)) {
    throw fail('INVALID_ARGUMENT', `${name} must be a bounded, nonempty string without control characters.`);
  }
}

function parse(body) {
  if (!object(body) || Object.keys(body).some((key) => !['method', 'params'].includes(key))
    || typeof body.method !== 'string' || !Object.hasOwn(fields, body.method)) {
    throw fail('INVALID_ARGUMENT', 'Expected a supported method and object parameters.');
  }
  const params = body.params === undefined ? {} : body.params;
  const allowed = fields[body.method];
  if (!object(params) || Object.keys(params).some((key) => !allowed.includes(key))
    || allowed.some((key) => !(body.method === 'devices.list' && key === 'platform') && !Object.hasOwn(params, key))) {
    throw fail('INVALID_ARGUMENT', 'Unexpected or missing request parameters.');
  }
  for (const name of ['client', 'binding', 'stream']) {
    if (name in params && (typeof params[name] !== 'string' || !/^[A-Za-z0-9_-]{43}$/u.test(params[name]))) {
      throw fail('INVALID_ARGUMENT', `${name} must be a service-issued token.`);
    }
  }
  for (const name of ['sessionId', 'device', 'avd', 'udid']) {
    if (name in params) boundedString(params[name], name, name === 'avd' ? 128 : 256);
  }
  if ('platform' in params && !['android', 'ios'].includes(params.platform)) {
    throw fail('INVALID_ARGUMENT', 'platform must be android or ios.');
  }
  if ('udid' in params && !/^[A-Fa-f0-9]{8}-[A-Fa-f0-9]{4}-[A-Fa-f0-9]{4}-[A-Fa-f0-9]{4}-[A-Fa-f0-9]{12}$/u.test(params.udid)) {
    throw fail('INVALID_ARGUMENT', 'udid must identify an iOS Simulator.');
  }
  if (['emulator.start', 'simulator.start'].includes(body.method) && params.consent !== true) {
    throw fail('PERMISSION_DENIED', 'Starting an emulator or simulator requires explicit consent.');
  }
  return { method: body.method, params };
}

/** One local Rust host, with opaque browser handles scoped to clients and DSH conversations. */
export class ConnectionService {
  #createBridge; #bridgeConfig; #validateSession; #options;
  #bridge; #starting; #closing; #clients = new Map(); #bindings = new Map();
  #tail = Promise.resolve(); #wireTail = Promise.resolve(); #pending = 0; #disposed = false; #disposing;
  #salt = token(); #timer;
  #previews;
  #platforms = new Map(); #platformRevision = 0; #platformEpoch = token();
  #connecting = new Set();
  #platformRequests = new Set(); #platformMutations = new Map();

  constructor({ createBridge, bridgeConfig, validateSession, requestTimeoutMs = 15_000,
    bootTimeoutMs = 130_000, heartbeatMs = 15_000, leaseTtlMs = 45_000,
    sweepIntervalMs = 1_000, maxClients = 32, previewTimeoutMs = 45_000,
    deviceAssets, videoMaxSize = 1280, videoBitRate = 4_000_000, videoMaxFps = 30,
    iosAvailable = process.platform === 'darwin' }) {
    if (typeof createBridge !== 'function' || typeof validateSession !== 'function') {
      throw fail('INVALID_CONFIG', 'Bridge creation and conversation validation are required.');
    }
    const options = { requestTimeoutMs, bootTimeoutMs, heartbeatMs, leaseTtlMs, sweepIntervalMs, maxClients, previewTimeoutMs };
    for (const [name, value] of Object.entries(options)) {
      if (!Number.isSafeInteger(value) || value < 1 || value > (name === 'maxClients' ? 32 : 600_000)) {
        throw fail('INVALID_CONFIG', `Invalid ${name}.`);
      }
    }
    if (heartbeatMs >= leaseTtlMs) throw fail('INVALID_CONFIG', 'Heartbeat must be shorter than the client lease TTL.');
    this.#createBridge = createBridge;
    this.#bridgeConfig = bridgeConfig;
    this.#validateSession = validateSession;
    if (typeof iosAvailable !== 'boolean') throw fail('INVALID_CONFIG', 'Invalid iOS availability.');
    this.#options = { ...options, iosAvailable };
    this.#previews = new PreviewPool({
      assetsDir: deviceAssets, maxSize: videoMaxSize, bitRate: videoBitRate, maxFps: videoMaxFps,
      rpc: (binding, method, params, timeoutMs) => this.#wire(binding.bridge, method, params, timeoutMs),
    });
    this.#timer = setInterval(() => { this.sweep().catch(() => {}); }, sweepIntervalMs);
    this.#timer.unref();
  }

  async handle(body, { signal } = {}) {
    try { return await this.#handle(body, signal); }
    catch (error) {
      if (error instanceof ServiceError) throw error;
      if (typeof error?.code === 'string' && typeof error?.message === 'string') {
        throw new ServiceError(error.code, error.message, error.hint);
      }
      throw fail('INTERNAL_ERROR', 'The mobile preview operation failed; check the host connection and retry.');
    }
  }

  async #handle(body, signal) {
    if (this.#disposed) throw fail('CLOSED', 'The mobile preview service is closed.');
    if (signal?.aborted) throw fail('ABORTED', 'The request was cancelled.');
    const { method, params } = parse(body);
    if (method === 'client.open') {
      await this.sweep();
      if (this.#disposed) throw fail('CLOSED', 'The mobile preview service is closed.');
      if (signal?.aborted) throw fail('ABORTED', 'The request was cancelled.');
      if (this.#clients.size >= this.#options.maxClients) throw fail('BUSY', 'Too many mobile preview clients.');
      const client = token();
      this.#clients.set(client, { token: client, expires: Date.now() + this.#options.leaseTtlMs,
        controller: new AbortController(), closed: false });
      return { client, host: hostname(), heartbeatMs: this.#options.heartbeatMs,
        leaseTtlMs: this.#options.leaseTtlMs, requestTimeoutMs: this.#options.requestTimeoutMs,
        bootTimeoutMs: this.#options.bootTimeoutMs, previewTimeoutMs: this.#options.previewTimeoutMs,
        capabilities: { video: this.#options.iosAvailable || Boolean(this.#previews.available), input: Boolean(this.#previews.available) } };
    }
    const client = this.#clients.get(params.client);
    if (!client || client.closed) throw fail('CLIENT_EXPIRED', 'The mobile preview client is no longer active.');
    if (Date.now() >= client.expires) {
      await this.#endClient(client);
      throw fail('CLIENT_EXPIRED', 'The mobile preview client heartbeat expired.');
    }
    if (method === 'client.close') {
      await this.#endClient(client);
      return { closed: true };
    }
    if (method === 'client.heartbeat') {
      client.expires = Date.now() + this.#options.leaseTtlMs;
      // Conversation checks do not wait behind a long-running emulator boot.
      await this.#audit(client, signal);
      return { alive: true };
    }
    if (method === 'preview.media') throw fail('INVALID_ARGUMENT', 'Use the binary media endpoint.');
    if (method === 'input.send' || method === 'preview.stop') {
      const binding = await this.#previewBinding(client, params.binding, signal);
      // Live control must not wait behind the globally serialized boot/discovery queue.
      return method === 'input.send'
        ? this.#previews.input(binding, params.stream, params.requests, signal)
        : this.#previews.stop(binding, params.stream);
    }
    if (method === 'platform.get' || method === 'platform.select') {
      return this.#platformOperation(params.sessionId, params.platform, 'user', signal, client);
    }
    const timeoutMs = ['emulator.start', 'simulator.start'].includes(method) ? this.#options.bootTimeoutMs
      : method === 'preview.start' ? this.#options.previewTimeoutMs : this.#options.requestTimeoutMs;
    return this.#schedule(client, signal, timeoutMs, async (ctx) => {
      if (method === 'devices.list') return this.#request(ctx, method, params.platform === undefined ? {} : { platform: params.platform });
      if (method === 'emulator.start') return this.#request(ctx, method, { avd: params.avd, consent: true });
      if (method === 'simulator.start') return this.#request(ctx, method, { udid: params.udid, consent: true });
      if (method === 'session.connect') return this.#connect(client, params, ctx);
      if (method === 'preview.start') {
        const binding = await this.#previewBinding(client, params.binding, ctx.signal);
        ctx.check();
        try {
          const result = await this.#previews.start(binding, { signal: ctx.signal });
          ctx.check();
          return result;
        } catch (error) {
          if (error?.code === 'PROTOCOL_ERROR') await this.#dropBridge(binding.bridge);
          if (ctx.signal.aborted) await this.#previews.stopBinding(binding);
          throw error;
        }
      }
      let binding;
      if (method === 'session.list') {
        binding = [...this.#bindings.values()].find((item) => item.client === client && item.sessionId === params.sessionId);
        try { await this.#validate(params.sessionId, ctx.signal); }
        catch (error) {
          if (binding && missingSession(error)) await this.#release(binding);
          throw error;
        }
        ctx.check();
        if (!binding) return null;
      } else {
        binding = this.#bindings.get(params.binding);
        if (!binding || binding.client !== client) throw fail('STALE_SESSION', 'The device binding is not owned by this client.');
      }
      if (method === 'session.disconnect') {
        const session = await this.#release(binding);
        return session ? { binding: binding.token, session } : null;
      }
      try {
        await this.#validate(binding.sessionId, ctx.signal);
      } catch (error) {
        if (missingSession(error)) await this.#release(binding);
        throw error;
      }
      ctx.check();
      let session;
      try { session = await this.#request(ctx, 'session.status', this.#lease(binding), binding.bridge); }
      catch (error) {
        // Rust drops the lease on every completed probe failure, including unplug errors.
        // Cancellation before the request is sent must preserve an existing valid binding.
        if (ctx.failedMethod === 'session.status') {
          this.#bindings.delete(binding.token);
          await this.#previews.stopBinding(binding);
        }
        throw error;
      }
      ctx.check();
      binding.session = session;
      return { binding: binding.token, session };
    });
  }

  /** Agent requests carry the live DSH session identity, never a browser binding. */
  requestPlatform(sessionId, platform, { signal } = {}) {
    if (!['android', 'ios'].includes(platform)) {
      return Promise.reject(fail('INVALID_ARGUMENT', 'platform must be android or ios.'));
    }
    return this.#platformOperation(sessionId, platform, 'agent', signal);
  }

  async #platformOperation(sessionId, platform, source, signal, client) {
    boundedString(sessionId, 'sessionId', 256);
    const controller = new AbortController();
    if (this.#disposed) throw fail('CLOSED', 'The mobile preview service is closed.');
    if (this.#platformRequests.size >= 64) throw fail('BUSY', 'Too many pending platform requests.');
    this.#platformRequests.add(controller);
    if (platform !== undefined) {
      this.#platformMutations.get(sessionId)?.abort(fail('ABORTED', 'A newer platform choice superseded this request.'));
      this.#platformMutations.set(sessionId, controller);
    }
    const cancel = () => controller.abort(fail('ABORTED', 'The platform request was cancelled.'));
    const expired = () => controller.abort(fail('CLIENT_EXPIRED', 'The mobile preview client is no longer active.'));
    const check = () => {
      if (this.#disposed) throw fail('CLOSED', 'The mobile preview service is closed.');
      if (controller.signal.aborted) throw controller.signal.reason;
      if (client && (client.closed || Date.now() >= client.expires)) {
        throw fail('CLIENT_EXPIRED', 'The mobile preview client is no longer active.');
      }
    };
    signal?.addEventListener('abort', cancel, { once: true });
    client?.controller.signal.addEventListener('abort', expired, { once: true });
    if (signal?.aborted) cancel();
    if (client?.closed) expired();
    const timer = setTimeout(() => controller.abort(fail('TIMEOUT', 'The platform request timed out.')), this.#options.requestTimeoutMs);
    try {
      check();
      await this.#validate(sessionId, controller.signal);
      check();
      const previous = this.#platforms.get(sessionId);
      if (platform !== undefined) {
        if (this.#connecting.has(sessionId)) {
          throw new ServiceError('BUSY', 'A device connection is in progress.', 'Wait for the connection to finish before choosing a platform.');
        }
        const connections = [...this.#bindings.values()].filter(binding => binding.sessionId === sessionId);
        if (connections.some(binding => binding.session.device.platform !== platform)) {
          throw new ServiceError('BUSY', 'This conversation already has a device on another platform.',
            'Disconnect the current device before choosing another platform.');
        }
        const selection = { sessionId, platform, source, revision: ++this.#platformRevision,
          epoch: this.#platformEpoch, available: platform === 'android' || this.#options.iosAvailable };
        this.#platforms.delete(sessionId);
        // Selection is transient host state; keep a bounded working set of conversations.
        if (this.#platforms.size >= 256) this.#platforms.delete(this.#platforms.keys().next().value);
        this.#platforms.set(sessionId, selection);
        return { ...selection };
      }
      if (previous) {
        this.#platforms.delete(sessionId);
        this.#platforms.set(sessionId, previous);
        return { ...previous };
      }
      return { sessionId, platform: null, source: null, revision: this.#platformRevision,
        epoch: this.#platformEpoch, available: false };
    } catch (error) {
      if (missingSession(error)) this.#platforms.delete(sessionId);
      throw error;
    } finally {
      clearTimeout(timer);
      signal?.removeEventListener('abort', cancel);
      client?.controller.signal.removeEventListener('abort', expired);
      this.#platformRequests.delete(controller);
      if (this.#platformMutations.get(sessionId) === controller) this.#platformMutations.delete(sessionId);
    }
  }

  async #connect(client, params, ctx) {
    this.#connecting.add(params.sessionId);
    try { return await this.#connectDevice(client, params, ctx); }
    finally { this.#connecting.delete(params.sessionId); }
  }

  async #connectDevice(client, params, ctx) {
    await this.#validate(params.sessionId, ctx.signal);
    ctx.check();
    const chosenPlatform = this.#platforms.get(params.sessionId)?.platform;
    if ((chosenPlatform === 'ios' || params.device.startsWith('ios:')) && !this.#options.iosAvailable) {
      throw new ServiceError('UNSUPPORTED', 'iOS Simulator preview requires a macOS host.',
        'Connect to a macOS host with Xcode installed, or select Android.');
    }
    if (chosenPlatform && !params.device.startsWith(`${chosenPlatform}:`)) {
      throw fail('INVALID_ARGUMENT', 'The device must match the platform selected for this conversation.');
    }
    if (this.#bindings.size >= 64) throw fail('BUSY', 'Too many connected device sessions.');
    const owner = `dsh:${createHash('sha256').update(`${this.#salt}\0${params.sessionId}`).digest('hex')}`;
    const session = await this.#request(ctx, 'session.connect', { owner, device: params.device });
    const binding = { client, sessionId: params.sessionId, session, bridge: ctx.bridge, owner };
    try {
      if (!object(session) || typeof session.id !== 'string' || session.owner !== owner
        || !Number.isSafeInteger(session.generation) || session.generation < 1) {
        await this.#dropBridge(ctx.bridge);
        throw fail('PROTOCOL_ERROR', 'The host returned an invalid device session.');
      }
      ctx.check();
      await this.#validate(params.sessionId, ctx.signal);
      ctx.check();
      binding.token = token();
      this.#bindings.set(binding.token, binding);
      return { binding: binding.token, session };
    } catch (error) {
      await this.#release(binding);
      throw error;
    }
  }

  #lease(binding) {
    return { owner: binding.owner, session: binding.session.id, generation: binding.session.generation };
  }

  async #release(binding) {
    if (binding.token) this.#bindings.delete(binding.token);
    try { await this.#previews.stopBinding(binding); }
    catch (error) {
      // A lost browser binding must never leave a live, unreachable Rust lease.
      await this.#dropBridge(binding.bridge);
      throw error;
    }
    if (binding.bridge.closed) return null;
    try {
      return await this.#wire(binding.bridge, 'session.disconnect', this.#lease(binding), this.#options.requestTimeoutMs);
    } catch (error) {
      if (error?.code === 'STALE_SESSION') return null;
      await this.#dropBridge(binding.bridge);
      throw error;
    }
  }

  async #previewBinding(client, key, signal) {
    const binding = this.#bindings.get(key);
    if (!binding || binding.client !== client) throw fail('STALE_SESSION', 'The device binding is not owned by this client.');
    try { await this.#validate(binding.sessionId, signal); }
    catch (error) {
      if (missingSession(error)) await this.#release(binding);
      throw error;
    }
    if (signal?.aborted) throw fail('ABORTED', 'The preview request was cancelled.');
    if (this.#disposed || client.closed || Date.now() >= client.expires
      || this.#bindings.get(key) !== binding || binding.bridge.closed) {
      throw fail('STALE_SESSION', 'The device connection is no longer active.');
    }
    return binding;
  }

  /** Return media only after authorizing the same client and conversation binding. */
  async openMedia(params, { signal } = {}) {
    parse({ method: 'preview.media', params });
    const client = this.#clients.get(params.client);
    if (!client || client.closed || Date.now() >= client.expires) {
      throw fail('CLIENT_EXPIRED', 'The mobile preview client is no longer active.');
    }
    const binding = await this.#previewBinding(client, params.binding, signal);
    return this.#previews.media(binding, params.stream, signal);
  }

  async #validate(sessionId, signal) {
    if (signal?.aborted) throw signal.reason;
    let cancel;
    const aborted = new Promise((_, reject) => { cancel = () => reject(signal.reason); });
    signal?.addEventListener('abort', cancel, { once: true });
    try {
      const valid = await Promise.race([Promise.resolve().then(() => this.#validateSession(sessionId, signal)), aborted]);
      if (!valid) throw fail('NOT_FOUND', 'The DSH conversation no longer exists or is not accessible.');
    } finally {
      signal?.removeEventListener('abort', cancel);
    }
  }

  async #audit(client, signal) {
    const controller = new AbortController();
    const timer = setTimeout(() => controller.abort(fail('TIMEOUT', 'Conversation validation timed out.')), this.#options.requestTimeoutMs);
    const cancel = () => controller.abort(fail('ABORTED', 'The heartbeat was cancelled.'));
    signal?.addEventListener('abort', cancel, { once: true });
    let rejectAbort;
    const aborted = new Promise((_, reject) => { rejectAbort = reject; });
    const onAbort = () => rejectAbort(controller.signal.reason);
    controller.signal.addEventListener('abort', onAbort, { once: true });
    try {
      const work = Promise.all([...this.#bindings.values()].filter((binding) => binding.client === client).map(async (binding) => {
        try { await this.#validate(binding.sessionId, controller.signal); }
        catch (error) {
          if (!missingSession(error)) throw error;
          // Forget the handle immediately, but do not hold heartbeats behind a boot.
          // The wire queue releases it when the host is available; failure closes that host.
          this.#release(binding).catch(() => {});
        }
      }));
      await Promise.race([work, aborted]);
    } finally {
      clearTimeout(timer);
      signal?.removeEventListener('abort', cancel);
      controller.signal.removeEventListener('abort', onAbort);
    }
  }

  async #request(ctx, method, params, bridge) {
    ctx.check();
    ctx.bridge = bridge ?? await this.#getBridge();
    ctx.check();
    return this.#wire(ctx.bridge, method, params, ctx.timeoutMs, ctx);
  }

  #wire(bridge, method, params, timeoutMs, ctx) {
    const request = this.#wireTail.then(async () => {
      ctx?.check();
      if (ctx) { ctx.sent = true; ctx.method = method; }
      try { return await bridge.request(method, params, { timeoutMs }); }
      catch (error) { if (ctx) ctx.failedMethod = method; throw error; }
      finally { if (ctx) ctx.sent = false; }
    });
    this.#wireTail = request.catch(() => {});
    return request;
  }

  async #getBridge() {
    await this.#closing;
    if (this.#disposed) throw fail('CLOSED', 'The mobile preview service is closed.');
    if (this.#bridge && !this.#bridge.closed) return this.#bridge;
    if (!this.#starting) {
      this.#starting = Promise.resolve().then(() => this.#createBridge(this.#bridgeConfig)).then(async (bridge) => {
        if (this.#disposed) {
          await bridge.close();
          throw fail('CLOSED', 'The mobile preview service is closed.');
        }
        bridge.on('invalidated', () => {
          if (this.#bridge === bridge) {
            this.#bridge = undefined;
            this.#closing = Promise.resolve(bridge.close()).catch(() => {});
          }
          for (const [key, binding] of this.#bindings) if (binding.bridge === bridge) {
            this.#bindings.delete(key);
            this.#previews.stopBinding(binding).catch(() => {});
          }
        });
        this.#bridge = bridge;
        return bridge;
      }).finally(() => { this.#starting = undefined; });
    }
    return this.#starting;
  }

  async #dropBridge(bridge) {
    if (!bridge) return;
    if (this.#bridge === bridge) this.#bridge = undefined;
    for (const [key, binding] of this.#bindings) if (binding.bridge === bridge) {
      this.#bindings.delete(key);
      this.#previews.stopBinding(binding).catch(() => {});
    }
    const closing = Promise.resolve(bridge.close());
    this.#closing = closing.catch(() => {});
    await closing;
  }

  #schedule(client, signal, timeoutMs, fn) {
    if (this.#pending >= 64) return Promise.reject(fail('BUSY', 'Too many pending mobile preview requests.'));
    this.#pending++;
    const controller = new AbortController();
    const deadline = Date.now() + timeoutMs;
    const ctx = { signal: controller.signal, bridge: undefined, timeoutMs, check: () => {
      if (controller.signal.aborted) throw controller.signal.reason;
      if (this.#disposed || client.closed) throw fail('CLIENT_EXPIRED', 'The client is no longer active.');
      if (Date.now() >= client.expires) {
        controller.abort(fail('CLIENT_EXPIRED', 'The client heartbeat expired.'));
        throw controller.signal.reason;
      }
      if (Date.now() >= deadline) throw fail('TIMEOUT', 'The mobile preview request timed out.');
    } };
    const cancel = () => controller.abort(fail('ABORTED', 'The request was cancelled.'));
    const expire = () => controller.abort(fail('CLIENT_EXPIRED', 'The client is no longer active.'));
    signal?.addEventListener('abort', cancel, { once: true });
    client.controller.signal.addEventListener('abort', expire, { once: true });
    const timer = setTimeout(() => controller.abort(fail('TIMEOUT', 'The mobile preview request timed out.')), timeoutMs);
    let active = false;
    let cleanup = Promise.resolve();
    let rejectAbort;
    const aborted = new Promise((_, reject) => { rejectAbort = reject; });
    const onAbort = () => {
      // Ordinary cancellation lets a consented boot finish and releases a late connect.
      // A timed-out connect has no trustworthy receipt, so its host must be stopped.
      cleanup = active && ctx.sent && !['emulator.start', 'simulator.start'].includes(ctx.method)
        && controller.signal.reason.code === 'TIMEOUT' ? this.#dropBridge(ctx.bridge) : Promise.resolve();
      cleanup.then(() => rejectAbort(controller.signal.reason), () => rejectAbort(controller.signal.reason));
    };
    controller.signal.addEventListener('abort', onAbort, { once: true });
    const work = this.#tail.then(() => {
      ctx.check(); active = true;
      const executing = (async () => {
        try {
          const result = await fn(ctx);
          ctx.check();
          return result;
        } catch (error) {
          if (controller.signal.aborted) {
            await cleanup.catch(() => {});
            throw controller.signal.reason;
          }
          throw error;
        }
      })();
      return Promise.race([executing, aborted]);
    });
    const result = Promise.race([work, aborted]);
    // A queued cancellation must not let later work overtake a still-running boot.
    this.#tail = work.finally(() => { this.#pending--; }).catch(() => {});
    return result.finally(() => {
      clearTimeout(timer);
      signal?.removeEventListener('abort', cancel);
      client.controller.signal.removeEventListener('abort', expire);
      controller.signal.removeEventListener('abort', onAbort);
    });
  }

  async #endClient(client) {
    if (client.closed) return;
    client.closed = true;
    this.#clients.delete(client.token);
    client.controller.abort();
    await Promise.all([...this.#bindings.values()].filter((binding) => binding.client === client)
      .map((binding) => this.#release(binding).catch(() => {})));
  }

  async sweep() {
    await Promise.all([...this.#clients.values()].filter((client) => Date.now() >= client.expires)
      .map((client) => this.#endClient(client)));
  }

  dispose() {
    if (this.#disposing) return this.#disposing;
    this.#disposed = true;
    clearInterval(this.#timer);
    this.#platforms.clear();
    for (const controller of this.#platformRequests) controller.abort(fail('CLOSED', 'The mobile preview service is closed.'));
    this.#platformMutations.clear();
    this.#disposing = (async () => {
      await Promise.all([...this.#clients.values()].map((client) => this.#endClient(client)));
      await this.#previews.dispose();
      await this.#wireTail;
      await this.#dropBridge(this.#bridge);
      await this.#starting?.catch(() => {});
      await this.#closing;
    })();
    return this.#disposing;
  }
}
