import assert from 'node:assert/strict';
import { readFile } from 'node:fs/promises';
import test from 'node:test';
import vm from 'node:vm';

const artifact = await readFile(new URL('../client.js', import.meta.url), 'utf8');
const ID = '@mobile-dev-harness/dsh-mobile-preview';
const DEVICE = { id: 'android:phone', platform: 'android', kind: 'physical', state: 'online', name: 'Pixel', serial: 'phone', avd: null };
const flush = async () => { for (let i = 0; i < 8; i += 1) await new Promise(resolve => setImmediate(resolve)); };

function harness(timing = {}) {
  const requests = [];
  const slots = new Map();
  const effects = [];
  const intervals = new Map();
  const windowEvents = new Map();
  const documentEvents = new Map();
  const backend = new Map();
  let now = 1000;
  let nextTimer = 0;
  let latestTimeout = null;
  let clientNumber = 0;
  let handler;
  let plugin;
  let tab;
  let closeHandler;
  let mounted = 'chat-a';
  const opened = [];
  const React = {
    createElement: (type, props, ...children) => ({ type, props: { ...props, children } }),
    Fragment: 'fragment', useState: initial => [initial, () => {}], useEffect: () => {},
  };
  const store = {
    createSnapshotStore(initial) {
      let snapshot = initial;
      return { getSnapshot: () => snapshot, subscribe: () => () => {},
        update(fn) { snapshot = structuredClone(snapshot); fn(snapshot); } };
    },
  };
  async function dispatch(method, params) {
    if (handler) {
      const result = await handler(method, params);
      if (result !== undefined) return result;
    }
    switch (method) {
      case 'client.open': return { client: `client-${++clientNumber}`, host: 'dev-host', heartbeatMs: 15000, leaseTtlMs: 45000,
        requestTimeoutMs: 15000, bootTimeoutMs: 130000, ...timing, capabilities: { video: false, input: false } };
      case 'client.heartbeat': return { alive: true };
      case 'client.close': backend.clear(); return { closed: true };
      case 'devices.list': return { devices: [DEVICE], warnings: [] };
      case 'session.list': return backend.get(params.sessionId) || null;
      case 'session.connect': {
        const result = { binding: `binding-${params.sessionId}`, session: { id: 'lease', owner: 'opaque', generation: 1, state: 'transport_ready', device: { ...DEVICE, id: params.device } } };
        backend.set(params.sessionId, result);
        return result;
      }
      case 'session.status': return [...backend.values()].find(row => row.binding === params.binding) || null;
      case 'session.disconnect':
        for (const [id, row] of backend) if (row.binding === params.binding) backend.delete(id);
        return null;
      case 'emulator.start': return { ...DEVICE, id: 'android:emulator-5554', avd: params.avd };
      default: throw new Error(`Unexpected method ${method}`);
    }
  }
  const window = { __ModuleLoader__: { load(definition) {
    assert.equal(definition.id, ID);
    plugin = definition.factory(name => ({ react: React, '@deepseek-ai/dsh-client-store': store,
      '@deepseek-ai/dsh-client-ui-primitives': { Button: 'button', StateDot: 'state-dot' } })[name]);
  } }, addEventListener: (name, fn) => windowEvents.set(name, fn), removeEventListener: name => windowEvents.delete(name) };
  const document = { visibilityState: 'visible', addEventListener: (name, fn) => documentEvents.set(name, fn), removeEventListener: name => documentEvents.delete(name) };
  const sandbox = { window, document, AbortController, console,
    Date: class extends Date { static now() { return now; } },
    setTimeout: (_fn, ms) => { latestTimeout = ms; return ++nextTimer; }, clearTimeout: () => {},
    setInterval: (fn, ms) => { const id = ++nextTimer; intervals.set(id, { fn, ms }); return id; },
    clearInterval: id => intervals.delete(id),
    fetch: async (url, options) => {
      const { method, params } = JSON.parse(options.body);
      requests.push({ url, method, params, options, timeoutMs: latestTimeout });
      try {
        const result = await dispatch(method, params);
        return { ok: true, json: async () => ({ ok: true, result }) };
      }
      catch (error) { return { ok: true, json: async () => ({ ok: false, error }) }; }
    },
  };
  vm.runInNewContext(artifact, sandbox, { filename: 'client.js' });
  let dictionaries;
  const ctx = {
    effect(fn) { const dispose = fn(); effects.push(dispose); return dispose; },
    locale: { register(_name, copy) { dictionaries = copy; return () => {}; }, bind: () => key => dictionaries.en[key] },
    slots: { inject: (_name, fn) => fn(), register(options, component) { slots.set(options.name, { options, component }); return () => slots.delete(options.name); } },
    sidebarRightTabs: { register(definition) { tab = definition; return () => {}; } },
    sidebarRight: {
      mounted: { getSnapshot: () => mounted },
      openTab: kind => opened.push(kind),
      registerCloseHandler(_kind, fn) { closeHandler = fn; return () => {}; },
    },
  };
  plugin.apply(ctx);
  const body = slots.get('sidebar.right.pane.tab');
  return {
    requests, slots, opened, intervals, windowEvents, documentEvents, document, React, body,
    get tab() { return tab; },
    ui: id => body.options.inject(id),
    state: () => body.options.inject('chat-a').hooks.preview.getSnapshot(),
    setHandler: fn => { handler = fn; }, setMounted: id => { mounted = id; },
    advance: ms => { now += ms; },
    tick: async () => { for (const { fn } of intervals.values()) fn(); await flush(); },
    closeTab: id => closeHandler(id, {}),
    dispose: () => { for (const dispose of effects.reverse()) if (typeof dispose === 'function') dispose(); },
    t: key => dictionaries.en[key],
  };
}

test('registers shared header/composer entries and mounted-session guarded sidebar', () => {
  const app = harness();
  assert.equal(app.tab.id, ID);
  assert.equal(app.tab.kind, 'mobile-preview');
  assert.equal(app.tab.keepMounted, true);
  assert.equal(app.body.options.key, ID);
  assert.ok(app.slots.has('conversation.input.left'));
  const { open } = app.slots.get('conversation.session.header.utilities').options.inject();
  open('chat-b');
  assert.deepEqual(app.opened, []);
  open('chat-a');
  assert.deepEqual(app.opened, ['mobile-preview']);
  assert.equal(app.ui('chat-a'), app.ui('chat-a'));
  app.dispose();
});

test('chat-scoped bindings survive panel close and switching, heartbeat covers inactive chats', async () => {
  const app = harness();
  const a = app.ui('chat-a');
  const b = app.ui('chat-b');
  await a.activate();
  await a.connect(DEVICE.id);
  await b.connect('android:second');
  app.setMounted('chat-b');
  app.closeTab('chat-a');
  assert.equal(app.requests.filter(row => row.method === 'session.disconnect').length, 0);
  app.advance(15000);
  await app.tick();
  const statuses = app.requests.filter(row => row.method === 'session.status');
  assert.deepEqual(statuses.map(row => row.params.binding).sort(), ['binding-chat-a', 'binding-chat-b']);
  assert.equal(app.state().sessions['chat-a'].verified, true);
  await b.disconnect();
  assert.equal(app.state().sessions['chat-a'].binding, 'binding-chat-a');
  assert.equal(app.state().sessions['chat-b'].binding, null);
  const last = app.requests.findLast(row => row.method === 'session.disconnect');
  assert.deepEqual(last.params, { client: 'client-1', binding: 'binding-chat-b' });
  assert.ok(app.requests.every(row => row.url === 'api/mobile-preview/v1'));
  app.dispose();
});

test('background lease expiration immediately clears stale connections without reconnecting', async () => {
  const app = harness();
  const ui = app.ui('chat-a');
  await ui.connect(DEVICE.id);
  app.advance(46000);
  app.documentEvents.get('visibilitychange')();
  await flush();
  assert.equal(app.state().sessions['chat-a'].binding, null);
  assert.equal(app.state().sessions['chat-a'].notice, 'expired');
  assert.equal(app.requests.filter(row => row.method === 'session.connect').length, 1);
  await ui.activate();
  assert.equal(app.requests.filter(row => row.method === 'client.open').length, 2);
  assert.equal(app.state().sessions['chat-a'].binding, null);
  app.dispose();
});

test('stale binding is dropped and request failure cannot retain verified status', async () => {
  const app = harness();
  await app.ui('chat-a').connect(DEVICE.id);
  app.setHandler(method => {
    if (method === 'session.status') throw { code: 'STALE_SESSION', message: 'Device replaced', hint: 'Reconnect' };
  });
  await app.tick();
  assert.equal(app.state().sessions['chat-a'].binding, null);
  assert.equal(app.state().sessions['chat-a'].notice, 'stale');
  app.setHandler(undefined);
  await app.ui('chat-a').connect(DEVICE.id);
  app.setHandler(method => {
    if (method === 'session.status') throw { code: 'TOOL_ERROR', message: 'Transport unreachable', hint: 'Retry' };
  });
  await app.tick();
  assert.equal(app.state().sessions['chat-a'].verified, false);
  assert.equal(app.state().sessions['chat-a'].error.code, 'TOOL_ERROR');
  app.dispose();
});

test('boot is explicit, scoped to selected AVD, and reports busy failures without fake success', async () => {
  const app = harness();
  const ui = app.ui('chat-a');
  await ui.start('Chosen_AVD');
  assert.deepEqual(app.requests.find(row => row.method === 'emulator.start').params,
    { client: 'client-1', avd: 'Chosen_AVD', consent: true });
  app.setHandler(method => {
    if (method === 'session.connect') throw { code: 'BUSY', message: 'Device belongs to another chat', hint: 'Disconnect it there' };
  });
  await ui.connect(DEVICE.id);
  assert.equal(app.state().sessions['chat-a'].binding, null);
  assert.equal(app.state().sessions['chat-a'].error.code, 'BUSY');
  app.dispose();
});

test('plugin unload cleans root timer/listeners and closes even a late client.open', async () => {
  const app = harness();
  let finish;
  app.setHandler(method => method === 'client.open' ? new Promise(resolve => { finish = resolve; }) : undefined);
  const pending = app.ui('chat-a').activate();
  await flush();
  app.dispose();
  finish({ client: 'late-client', host: 'host', leaseTtlMs: 45000, heartbeatMs: 15000,
    requestTimeoutMs: 15000, bootTimeoutMs: 130000 });
  await pending;
  await flush();
  assert.equal(app.intervals.size, 0);
  assert.equal(app.windowEvents.size, 0);
  assert.equal(app.documentEvents.size, 0);
  assert.ok(app.requests.some(row => row.method === 'client.close' && row.params.client === 'late-client'));
  assert.equal(app.requests.filter(row => row.method === 'devices.list').length, 0);
});

test('panel renders real device choices and states clearly that preview/input are unavailable', async () => {
  const app = harness();
  const ui = app.ui('chat-a');
  await ui.activate();
  const tree = app.body.component({ ...ui, sessionId: 'chat-a',
    usePreview: selector => selector(app.state()),
    useTabInfo: () => ({ tab: { visible: true, signal: new AbortController().signal } }), t: app.t,
  });
  const rendered = JSON.stringify(tree);
  assert.ok(rendered.includes('dev-host'));
  assert.ok(rendered.includes('Pixel'));
  assert.ok(rendered.includes('Live preview and touch input are not available'));
  assert.ok(rendered.includes('"checked":false'));
  assert.ok(!rendered.includes('binding-chat'));
  app.dispose();
});

test('negotiated heartbeat cadence renews short leases before their deadline', async () => {
  const app = harness({ heartbeatMs: 5000, leaseTtlMs: 15000 });
  await app.ui('chat-a').connect(DEVICE.id);
  assert.deepEqual([...app.intervals.values()].map(timer => timer.ms), [5000]);
  for (let index = 0; index < 4; index += 1) {
    app.advance(5000);
    await app.tick();
  }
  assert.equal(app.requests.filter(row => row.method === 'client.heartbeat').length, 4);
  assert.equal(app.state().sessions['chat-a'].binding, 'binding-chat-a');
  app.dispose();
});

test('slow status revalidation never suppresses lease renewal or duplicates status work', async () => {
  const app = harness({ heartbeatMs: 5000, leaseTtlMs: 15000, requestTimeoutMs: 60000 });
  await app.ui('chat-a').connect(DEVICE.id);
  let finishStatus;
  app.setHandler(method => method === 'session.status' ? new Promise(resolve => { finishStatus = resolve; }) : undefined);
  for (let index = 0; index < 4; index += 1) {
    app.advance(5000);
    await app.tick();
  }
  assert.equal(app.requests.filter(row => row.method === 'client.heartbeat').length, 4);
  assert.equal(app.requests.filter(row => row.method === 'session.status').length, 1);
  assert.equal(app.state().sessions['chat-a'].binding, 'binding-chat-a');
  finishStatus(null);
  await flush();
  assert.equal(app.state().sessions['chat-a'].binding, null);
  app.dispose();
});

test('negotiated ordinary and boot deadlines retain a bounded network margin', async () => {
  const app = harness({ requestTimeoutMs: 60000, bootTimeoutMs: 300000 });
  await app.ui('chat-a').activate();
  await app.ui('chat-a').start('Chosen_AVD');
  const normal = app.requests.find(row => row.method === 'devices.list').timeoutMs;
  const boot = app.requests.find(row => row.method === 'emulator.start').timeoutMs;
  assert.ok(normal > 60000 && normal <= 65000);
  assert.ok(boot > 300000 && boot <= 305000);
  assert.equal(normal - 60000, boot - 300000);
  app.dispose();
});
