import assert from 'node:assert/strict';
import test from 'node:test';
import { API_PATH, apply, createHandler, resolveConfig, validateSession } from '../index.js';

const config = { executable: '/tmp/mpp' };
const request = body => new Request(`http://localhost${API_PATH}`, {
  method: 'POST', headers: { 'content-type': 'application/json' }, body,
});

test('configuration rejects unknown settings, relative tools and invalid timing relationships', () => {
  assert.equal(resolveConfig(config).leaseTtlMs, 45_000);
  for (const value of [null, [], {}, { executable: 'mpp' },
    { ...config, adb: 'adb' }, { ...config, secret: 'do-not-echo' },
    { ...config, bootTimeoutMs: 10_000 }, { ...config, heartbeatMs: 20_000 },
    { ...config, requestTimeoutMs: Infinity }, { ...config, maxClients: 1.5 }]) {
    assert.throws(() => resolveConfig(value), error => !error.message.includes('do-not-echo'));
  }
});

test('route passes bounded JSON and the request signal without caching device data', async () => {
  const body = { method: 'devices.list', params: { client: 'local' } };
  const req = request(JSON.stringify(body));
  const handler = createHandler({ handle: async (actual, context) => {
    assert.deepEqual(actual, body);
    assert.equal(context.signal, req.signal);
    return { devices: [], warnings: [] };
  } });
  const response = await handler(req);
  assert.equal(response.status, 200);
  assert.equal(response.headers.get('cache-control'), 'no-store');
  assert.deepEqual(await response.json(), { ok: true, result: { devices: [], warnings: [] } });
});

test('route refuses malformed, oversized, wrong-content-type and cancelled requests before dispatch', async () => {
  let calls = 0;
  const handler = createHandler({ handle: async () => { calls++; } });
  assert.equal((await handler(request('{'))).status, 400);
  assert.equal((await handler(request(' '.repeat(65_537)))).status, 400);
  assert.equal((await handler(new Request('http://localhost', {
    method: 'POST', headers: { 'content-type': 'application/jsonp' }, body: '{}',
  }))).status, 400);
  const controller = new AbortController();
  controller.abort();
  const cancelled = new Request('http://localhost', {
    method: 'POST', headers: { 'content-type': 'application/json' }, body: '{}', signal: controller.signal,
  });
  assert.equal((await handler(cancelled)).status, 408);
  assert.equal((await handler(new Request('http://localhost'))).status, 405);
  assert.equal(calls, 0);
});

test('route returns actionable service errors and redacts unexpected internal failures', async () => {
  const busy = createHandler({ handle: async () => {
    throw Object.assign(new Error('Device is in another conversation.'), { code: 'BUSY', hint: 'Disconnect there first.' });
  } });
  const response = await busy(request('{}'));
  assert.equal(response.status, 409);
  assert.equal((await response.json()).error.code, 'BUSY');
  const broken = createHandler({ handle: async () => {
    throw Object.assign(new Error('private-value'), { hint: 'private-value' });
  } });
  assert.ok(!(await (await broken(request('{}'))).text()).includes('private-value'));
});

test('conversation validation observes metadata, disposes it and preserves failure distinctions', async () => {
  let disposed = 0;
  const signal = new AbortController().signal;
  assert.equal(await validateSession({ observeSession: async (id, options) => {
    assert.equal(id, 'conversation');
    assert.deepEqual(options, { signal, projectionMode: 'none' });
    return { [Symbol.dispose]() { disposed++; } };
  } }, 'conversation', signal), true);
  assert.equal(disposed, 1);
  for (const [code, expected] of [
    ['SESSION_QUERY_SESSION_NOT_FOUND', 'NOT_FOUND'],
    ['SESSION_QUERY_ABORTED', 'ABORTED'], ['CORRUPT_LOG', 'SESSION_UNAVAILABLE'],
  ]) {
    await assert.rejects(validateSession({ observeSession: async () => {
      throw Object.assign(new Error('private-value'), { code });
    } }, 'conversation', signal), error => error.code === expected && !error.message.includes('private-value'));
  }
});

test('plugin registers only its authenticated DSH route and awaited lifecycle cleanup', async () => {
  const cleanup = [];
  let route;
  apply({
    sessionQuery: {},
    effect(setup) { cleanup.push(setup()); },
    connection: { fetch: { register(value) { route = value; } } },
  }, config);
  assert.equal(route.path, API_PATH);
  assert.deepEqual(route.methods, ['POST']);
  assert.equal(route.requestBody, 'buffered');
  const response = await route.fetch(request(JSON.stringify({ method: 'client.open', params: {} })));
  assert.equal(response.status, 200);
  assert.equal(typeof (await response.json()).result.client, 'string');
  await Promise.all(cleanup.map(dispose => dispose()));
});
