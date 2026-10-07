import { isAbsolute } from 'node:path';
import { createBridge } from './src/host/bridge.mjs';
import { ConnectionService } from './src/host/service.mjs';

export const name = 'mobile-preview';
export const inject = ['connection', 'sessionQuery'];
export const API_PATH = '/api/mobile-preview/v1';
const MAX_BODY_BYTES = 65_536;

/** Validate deployment settings before registering any routes or starting a child. */
export function resolveConfig(input = {}) {
  if (input === null || typeof input !== 'object' || Array.isArray(input)) {
    throw new Error('mobile-preview config must be an object');
  }
  const allowed = new Set(['executable', 'adb', 'emulator', 'requestTimeoutMs',
    'bootTimeoutMs', 'heartbeatMs', 'leaseTtlMs', 'sweepIntervalMs', 'maxClients']);
  if (Object.keys(input).some(key => !allowed.has(key))) {
    throw new Error('mobile-preview config contains an unknown setting');
  }
  for (const key of ['executable', 'adb', 'emulator']) {
    const value = input[key];
    if (value === undefined && key !== 'executable') continue;
    if (typeof value !== 'string' || !isAbsolute(value) || value.includes('\0')) {
      throw new Error(`mobile-preview ${key} must be an absolute path`);
    }
  }
  const result = {
    requestTimeoutMs: 15_000, bootTimeoutMs: 130_000, heartbeatMs: 15_000,
    leaseTtlMs: 45_000, sweepIntervalMs: 1_000, maxClients: 32, ...input,
  };
  const ranges = {
    requestTimeoutMs: [1_000, 60_000], bootTimeoutMs: [130_000, 300_000],
    heartbeatMs: [5_000, 60_000], leaseTtlMs: [15_000, 300_000],
    sweepIntervalMs: [100, 5_000], maxClients: [1, 32],
  };
  for (const [key, [min, max]] of Object.entries(ranges)) {
    if (!Number.isSafeInteger(result[key]) || result[key] < min || result[key] > max) {
      throw new Error(`mobile-preview ${key} must be an integer in ${min}..${max}`);
    }
  }
  if (result.leaseTtlMs < result.heartbeatMs * 3) {
    throw new Error('mobile-preview leaseTtlMs must cover at least three heartbeats');
  }
  return result;
}

/** Observe metadata without activating an agent or computing its projections. */
export async function validateSession(sessionQuery, sessionId, signal) {
  try {
    const observation = await sessionQuery.observeSession(sessionId, {
      signal, projectionMode: 'none',
    });
    try {
      return true;
    } finally {
      observation[Symbol.dispose]();
    }
  } catch (error) {
    if (error?.code === 'SESSION_QUERY_SESSION_NOT_FOUND') {
      throw failure('NOT_FOUND', 'This DSH conversation no longer exists.', 'Open an existing conversation and reconnect.');
    }
    if (signal?.aborted || error?.code === 'SESSION_QUERY_ABORTED') {
      throw failure('ABORTED', 'The conversation check was cancelled.', 'Retry from the current conversation.');
    }
    throw failure('SESSION_UNAVAILABLE', 'DSH could not read this conversation.', 'Check the DSH Host log and retry.');
  }
}

function failure(code, message, hint) {
  return Object.assign(new Error(message), { code, hint });
}

async function requestBody(request) {
  if (request.headers.get('content-type')?.split(';')[0].trim().toLowerCase() !== 'application/json') {
    throw failure('INVALID_ARGUMENT', 'A JSON request is required.', 'Use application/json with method and params.');
  }
  const reader = request.body?.getReader();
  if (!reader) throw failure('INVALID_ARGUMENT', 'The request is empty.', 'Provide method and params.');
  const chunks = [];
  let size = 0;
  try {
    while (true) {
      const { done, value } = await reader.read();
      if (done) break;
      size += value.byteLength;
      if (size > MAX_BODY_BYTES) {
        await reader.cancel();
        throw failure('INVALID_ARGUMENT', 'The request exceeds 64 KiB.', 'Send one bounded device operation.');
      }
      chunks.push(value);
    }
  } finally {
    reader.releaseLock();
  }
  try {
    return JSON.parse(new TextDecoder('utf-8', { fatal: true }).decode(Buffer.concat(chunks, size)));
  } catch {
    throw failure('INVALID_ARGUMENT', 'The request is not valid UTF-8 JSON.', 'Provide method and params as a JSON object.');
  }
}

/** DSH authenticates this exact route before it reaches this handler. */
export function createHandler(service) {
  return async request => {
    try {
      if (request.method !== 'POST') {
        return new Response(null, { status: 405, headers: { allow: 'POST' } });
      }
      const body = await requestBody(request);
      if (request.signal.aborted) throw failure('ABORTED', 'The request was cancelled.', 'Retry from the current conversation.');
      const result = await service.handle(body, { signal: request.signal });
      return Response.json({ ok: true, result }, { headers: { 'cache-control': 'no-store' } });
    } catch (error) {
      const known = typeof error?.code === 'string' && /^[A-Z][A-Z_]{0,63}$/.test(error.code);
      const code = known ? error.code : 'INTERNAL_ERROR';
      const status = ['NOT_FOUND', 'STALE_SESSION', 'CLIENT_EXPIRED'].includes(code) ? 404
        : ['PERMISSION_DENIED', 'INVALID_BINDING'].includes(code) ? 403
        : code === 'BUSY' ? 409 : code === 'INVALID_ARGUMENT' ? 400
        : code === 'ABORTED' ? 408 : 503;
      return Response.json({ ok: false, error: {
        code,
        message: known ? error.message : 'The mobile device service could not complete the request.',
        hint: known && typeof error?.hint === 'string' ? error.hint : 'Refresh the device panel and retry.',
      } }, { status, headers: { 'cache-control': 'no-store' } });
    }
  };
}

/** One service and Rust subprocess coordinate every chat in this DSH Host instance. */
export function apply(ctx, input) {
  const config = resolveConfig(input);
  const { executable, adb, emulator, ...policy } = config;
  const service = new ConnectionService({
    ...policy, createBridge, bridgeConfig: { executable, adb, emulator },
    validateSession: (sessionId, signal) => validateSession(ctx.sessionQuery, sessionId, signal),
  });
  ctx.effect(() => () => service.dispose(), 'mobile-preview: owned Rust host and sessions');
  ctx.connection.fetch.register({
    path: API_PATH, methods: ['POST'], requestBody: 'buffered', fetch: createHandler(service),
  });
}
