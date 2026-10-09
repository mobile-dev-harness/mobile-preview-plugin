import { spawn } from 'node:child_process';
import { EventEmitter } from 'node:events';
import { constants } from 'node:fs';
import { access, stat } from 'node:fs/promises';
import { isAbsolute } from 'node:path';

export const REQUEST_LIMIT = 65_536;
export const RESPONSE_LIMIT = 1_048_576;
export const DEFAULT_TIMEOUT_MS = 15_000;
export const BOOT_TIMEOUT_MS = 130_000;
export const EXPECTED_METHODS = Object.freeze([
  'hello', 'devices.list', 'emulator.start', 'simulator.start', 'session.connect',
  'session.status', 'session.disconnect', 'preview.start', 'preview.stop', 'input.send',
]);

const MAX_PENDING = 64;
const MAX_TIMEOUT_MS = 600_000;
const ENVIRONMENT_KEYS = new Set([
  'PATH', 'HOME', 'USERPROFILE', 'ANDROID_HOME', 'ANDROID_SDK_ROOT', 'JAVA_HOME',
  'TMP', 'TEMP', 'TMPDIR', 'LANG', 'LC_ALL', 'SYSTEMROOT', 'WINDIR', 'PATHEXT',
  'ADB_SERVER_HOST', 'ADB_SERVER_PORT', 'ADB_SERVER_SOCKET',
  'ANDROID_ADB_SERVER_PORT', 'ANDROID_ADB_SERVER_ADDRESS',
]);

export class BridgeError extends Error {
  constructor(code, message, hint = 'Reconnect the mobile preview service and retry.') {
    super(message);
    this.name = 'BridgeError';
    this.code = code;
    this.hint = hint;
  }
}

function failure(code, message) {
  const hints = {
    INVALID_CONFIG: 'Configure absolute executable paths in the host plugin settings.',
    INVALID_ARGUMENT: 'Correct the request parameters and retry.',
    BUSY: 'Wait for pending requests to complete before sending more work.',
  };
  return new BridgeError(code, message, hints[code]);
}

function record(value) {
  return value !== null && typeof value === 'object'
    && [Object.prototype, null].includes(Object.getPrototypeOf(value));
}

function checkGenerations(value) {
  if (value === null || typeof value !== 'object') return;
  for (const [key, item] of Object.entries(value)) {
    if (key === 'generation' && (!Number.isSafeInteger(item) || item < 1)) {
      throw failure('PROTOCOL_ERROR', 'A session generation cannot be represented safely.');
    }
    checkGenerations(item);
  }
}

async function executablePath(value, field) {
  if (typeof value !== 'string' || !isAbsolute(value) || value.includes('\0')) {
    throw failure('INVALID_CONFIG', `${field} must be an absolute executable path.`);
  }
  try {
    if (!(await stat(value)).isFile()) throw new Error('not a file');
    await access(value, constants.X_OK);
  } catch {
    throw failure('INVALID_CONFIG', `${field} must point to an existing executable file.`);
  }
  return value;
}

/** Launch one local host. No executable paths or environment values come from requests. */
export async function createBridge(config) {
  if (!record(config) || Object.keys(config).some((key) => !['executable', 'adb', 'emulator', 'xcrun', 'shutdownTimeoutMs'].includes(key))) {
    throw failure('INVALID_CONFIG', 'Expected executable and optional adb/emulator/xcrun paths.');
  }
  const executable = await executablePath(config.executable, 'executable');
  const shutdownTimeoutMs = config.shutdownTimeoutMs ?? 30_000;
  if (!Number.isSafeInteger(shutdownTimeoutMs) || shutdownTimeoutMs < 1 || shutdownTimeoutMs > 60_000) {
    throw failure('INVALID_CONFIG', 'shutdownTimeoutMs must be an integer in 1..60000.');
  }
  const args = [];
  for (const key of ['adb', 'emulator', 'xcrun']) {
    if (config[key] !== undefined) args.push(`--${key}`, await executablePath(config[key], key));
  }
  args.push('serve', '--stdio');
  const env = Object.fromEntries(Object.entries(process.env).filter(([key]) => ENVIRONMENT_KEYS.has(key.toUpperCase())));
  const bridge = new MppBridge(executable, args, env, shutdownTimeoutMs);
  try {
    const hello = await bridge.request('hello');
    if (!record(hello) || hello.control_protocol !== 'mpp/v1'
      || typeof hello.version !== 'string' || hello.version.length === 0
      || !Array.isArray(hello.methods)
      || !EXPECTED_METHODS.every((method) => hello.methods.includes(method))) {
      throw failure('PROTOCOL_ERROR', 'MPP does not provide the required mpp/v1 methods.');
    }
    if (bridge.closed) throw failure('PROTOCOL_ERROR', 'MPP closed during initialization.');
    bridge.hello = Object.freeze(hello);
    return bridge;
  } catch (error) {
    await bridge.close();
    throw error;
  }
}

/** One bridge owns one subprocess and all leases within it. */
export class MppBridge extends EventEmitter {
  #child;
  #pending = new Map();
  #nextId = 0n;
  #buffer = Buffer.alloc(0);
  #closed = false;
  #startup = true;
  #exited;
  #killTimer;
  #shutdownTimeoutMs;

  constructor(executable, args, env, shutdownTimeoutMs = 30_000) {
    super();
    this.#shutdownTimeoutMs = shutdownTimeoutMs;
    this.#child = spawn(executable, args, { env, shell: false, windowsHide: true, stdio: ['pipe', 'pipe', 'pipe'] });
    this.#exited = new Promise((resolve) => {
      this.#child.once('close', () => {
        clearTimeout(this.#killTimer);
        resolve();
      });
    });
    this.#child.stdout.on('data', (chunk) => this.#receive(chunk));
    // Drain diagnostics without retaining or forwarding potential secrets.
    this.#child.stderr.on('data', () => {});
    this.#child.stdin.on('error', () => this.#fail(failure('HOST_EXITED', 'The MPP input channel closed.')));
    this.#child.once('error', () => this.#fail(failure('HOST_EXITED', 'The MPP process could not start.')));
    this.#child.once('exit', () => {
      this.#fail(failure('HOST_EXITED', 'The MPP process exited; existing device sessions are invalid.'));
    });
    this.#child.stdout.once('end', () => {
      if (!this.#closed) this.#fail(failure('PROTOCOL_ERROR', 'The MPP response stream ended unexpectedly.'));
    });
  }

  get closed() { return this.#closed; }

  /** Host errors reject this call; transport errors invalidate every call and lease. */
  request(method, params = {}, options = {}) {
    if (this.#closed) return Promise.reject(failure('CLOSED', 'The MPP bridge is closed.'));
    if (!EXPECTED_METHODS.includes(method) || !record(params) || !record(options)
      || Object.keys(options).some((key) => key !== 'timeoutMs')) {
      return Promise.reject(failure('INVALID_ARGUMENT', 'Expected a supported method, object parameters, and timeoutMs only.'));
    }
    const boot = ['emulator.start', 'simulator.start'].includes(method);
    const timeoutMs = options.timeoutMs ?? (boot ? BOOT_TIMEOUT_MS : DEFAULT_TIMEOUT_MS);
    if (!Number.isSafeInteger(timeoutMs) || timeoutMs < 1 || timeoutMs > MAX_TIMEOUT_MS
      || (boot && timeoutMs < BOOT_TIMEOUT_MS)) {
      return Promise.reject(failure('INVALID_ARGUMENT', 'Invalid timeout; device startup requires at least 130000 ms.'));
    }
    if (this.#pending.size >= MAX_PENDING) {
      return Promise.reject(failure('BUSY', 'Too many pending MPP requests.'));
    }
    let line;
    const id = (++this.#nextId).toString();
    try {
      checkGenerations(params);
      line = JSON.stringify({ id, method, params }, (_key, value) => {
        if (typeof value === 'number' && !Number.isFinite(value)) throw new Error('nonfinite number');
        if (typeof value === 'undefined' || typeof value === 'function' || typeof value === 'symbol') throw new Error('not JSON');
        return value;
      });
      if (Buffer.byteLength(line) > REQUEST_LIMIT) throw new Error('oversized request');
    } catch {
      return Promise.reject(failure('INVALID_ARGUMENT', 'Request must be bounded JSON with safely represented generations.'));
    }
    return new Promise((resolve, reject) => {
      const timer = setTimeout(() => {
        this.#fail(failure('TIMEOUT', 'An MPP request timed out; the host is stopping and device sessions are invalid.'));
      }, timeoutMs);
      this.#pending.set(id, { resolve, reject, timer, method });
      this.#child.stdin.write(`${line}\n`, (error) => {
        if (error) this.#fail(failure('HOST_EXITED', 'The MPP request could not be delivered.'));
      });
    });
  }

  async close() {
    this.#fail(failure('CLOSED', 'The MPP bridge was closed; device sessions are invalid.'));
    await this.#exited;
  }

  #fail(error) {
    if (this.#closed) return;
    this.#closed = true;
    for (const pending of this.#pending.values()) {
      clearTimeout(pending.timer);
      pending.reject(error);
    }
    this.#pending.clear();
    this.#buffer = Buffer.alloc(0);
    this.#child.stdin.end();
    // Keep draining output while Rust cancels startup and releases device resources.
    // SIGTERM is handled by the Rust host; SIGKILL is only a final bounded fallback.
    if (this.#child.exitCode === null && this.#child.signalCode === null) {
      this.#child.kill('SIGTERM');
      this.#killTimer = setTimeout(() => this.#child.kill('SIGKILL'), this.#shutdownTimeoutMs);
      this.#killTimer.unref();
    }
    this.emit('invalidated', error);
  }

  #receive(chunk) {
    if (this.#closed) return;
    let start = 0;
    while (start < chunk.length && !this.#closed) {
      const newline = chunk.indexOf(10, start);
      const end = newline < 0 ? chunk.length : newline;
      const part = chunk.subarray(start, end);
      if (this.#buffer.length + part.length > RESPONSE_LIMIT) {
        this.#fail(failure('PROTOCOL_ERROR', 'An MPP response exceeded the size limit.'));
        return;
      }
      this.#buffer = Buffer.concat([this.#buffer, part]);
      if (newline < 0) return;
      const line = this.#buffer;
      this.#buffer = Buffer.alloc(0);
      this.#response(line);
      start = newline + 1;
    }
  }

  #response(line) {
    try {
      const response = JSON.parse(new TextDecoder('utf-8', { fatal: true, ignoreBOM: true }).decode(line));
      checkGenerations(response);
      if (!record(response) || response.schema !== 'mpp/v1'
        || typeof response.ok !== 'boolean'
        || !Object.hasOwn(response, 'result') || !Object.hasOwn(response, 'error')
        || (response.ok ? response.error !== null || response.result === null : response.result !== null || !record(response.error))) {
        throw new Error('invalid envelope');
      }
      if (!response.ok && !['code', 'message', 'hint'].every((key) => typeof response.error[key] === 'string' && response.error[key].length > 0)) {
        throw new Error('invalid error');
      }
      // Tool discovery can fail before Rust reads hello, so its fatal receipt has no request ID.
      if (response.id === null && !response.ok && this.#startup && this.#pending.size === 1
        && this.#pending.values().next().value.method === 'hello'
        && Object.keys(response).length === 5
        && Object.keys(response.error).length === 3) {
        this.#fail(new BridgeError(response.error.code, response.error.message, response.error.hint));
        return;
      }
      if (typeof response.id !== 'string') throw new Error('invalid response ID');
      const pending = this.#pending.get(response.id);
      if (!pending) throw new Error('unexpected response ID');
      if (pending.method === 'hello') this.#startup = false;
      this.#pending.delete(response.id);
      clearTimeout(pending.timer);
      if (response.ok) pending.resolve(response.result);
      else pending.reject(new BridgeError(response.error.code, response.error.message, response.error.hint));
    } catch {
      this.#fail(failure('PROTOCOL_ERROR', 'The MPP process returned an invalid or unsafe response.'));
    }
  }
}
