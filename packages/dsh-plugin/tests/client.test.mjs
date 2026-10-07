import assert from 'node:assert/strict';
import { readFile } from 'node:fs/promises';
import test from 'node:test';
import vm from 'node:vm';

const artifact = await readFile(new URL('../client.js', import.meta.url), 'utf8');
const ID = '@mobile-dev-harness/dsh-mobile-preview';
const DEVICE = { id: 'android:phone', platform: 'android', kind: 'physical', state: 'online', name: 'Pixel', serial: 'phone', avd: null };
const flush = async () => { for (let i = 0; i < 8; i += 1) await new Promise(resolve => setImmediate(resolve)); };

function harness(timing = {}, browser = {}, services = {}) {
  const requests = [];
  const slots = new Map();
  const effects = [];
  const intervals = new Map();
  const timeouts = new Map();
  const windowEvents = new Map();
  const documentEvents = new Map();
  const backend = new Map();
  let now = 1000;
  let nextTimer = 0;
  let latestTimeout = null;
  let clientNumber = 0;
  let handler;
  let mediaHandler;
  let plugin;
  let tab;
  let closeHandler;
  let mounted = 'chat-a';
  const opened = [];
  const React = {
    // React consumes key for reconciliation instead of forwarding it as a component prop.
    createElement: (type, props, ...children) => ({ type, key: props?.key ?? null,
      props: { ...Object.fromEntries(Object.entries(props || {}).filter(([name]) => name !== 'key')), children } }),
    Fragment: 'fragment', useState: initial => [initial, () => {}], useRef: initial => ({ current: initial }), useEffect: () => {},
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
        requestTimeoutMs: 15000, bootTimeoutMs: 130000, previewTimeoutMs: 45000, ...timing, capabilities: { video: false, input: false } };
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
      case 'preview.stop': return { stopped: true };
      case 'input.send': return { replies: params.requests.map(item => ({ seq: item.seq, ok: true })) };
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
  const sandbox = { window, document, AbortController, console, Uint8Array, DataView, queueMicrotask, ...browser,
    Date: class extends Date { static now() { return now; } },
    setTimeout: (fn, ms) => { latestTimeout = ms; const id = ++nextTimer; timeouts.set(id, { fn, ms }); return id; }, clearTimeout: id => timeouts.delete(id),
    setInterval: (fn, ms) => { const id = ++nextTimer; intervals.set(id, { fn, ms }); return id; },
    clearInterval: id => intervals.delete(id),
    fetch: async (url, options) => {
      if (url.endsWith('/media')) { requests.push({ url, media: JSON.parse(options.body), options }); return mediaHandler(options); }
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
    layout: services.layout,
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
    requests, slots, opened, intervals, timeouts, windowEvents, documentEvents, document, React, body,
    get tab() { return tab; },
    get injectedServices() { return plugin.inject; },
    ui: id => body.options.inject(id),
    state: () => body.options.inject('chat-a').hooks.preview.getSnapshot(),
    setHandler: fn => { handler = fn; }, setMediaHandler: fn => { mediaHandler = fn; }, setMounted: id => { mounted = id; },
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
    requestTimeoutMs: 15000, bootTimeoutMs: 130000, previewTimeoutMs: 45000 });
  await pending;
  await flush();
  assert.equal(app.intervals.size, 0);
  assert.equal(app.windowEvents.size, 0);
  assert.equal(app.documentEvents.size, 0);
  assert.ok(app.requests.some(row => row.method === 'client.close' && row.params.client === 'late-client'));
  assert.equal(app.requests.filter(row => row.method === 'devices.list').length, 0);
});

test('panel renders real device choices and explains the preview backend requirement', async () => {
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
  assert.ok(rendered.includes('Preview requires the configured Android capture backend'));
  assert.ok(rendered.includes('"checked":false'));
  assert.ok(!rendered.includes('binding-chat'));
  app.dispose();
});

test('connected panel gives the viewport remaining height and keeps controls outside collapsed diagnostics', async () => {
  const app = harness(), ui = app.ui('chat-a');
  app.setHandler(method => method === 'devices.list' ? {
    devices: [DEVICE, ...Array.from({ length: 4 }, (_, index) => ({ ...DEVICE, id: `avd:${index}`, name: `Unused AVD ${index}`, state: 'stopped' }))],
    warnings: ['A discovery diagnostic'],
  } : undefined);
  await ui.activate(); await ui.connect(DEVICE.id);
  const render = status => app.body.component({ ...ui, sessionId: 'chat-a',
    usePreview: selector => {
      const state = structuredClone(app.state());
      state.sessions['chat-a'].preview = { status, error: null };
      return selector(state);
    },
    useTabInfo: () => ({ tab: { visible: true, signal: new AbortController().signal } }), t: app.t,
  });
  function nodes(value) {
    if (Array.isArray(value)) return value.flatMap(nodes);
    if (!value || typeof value !== 'object') return [];
    return [value, ...nodes(value.props?.children)];
  }
  const tree = render('live'), elements = nodes(tree);
  assert.equal(tree.props.style.overflow, 'hidden');
  assert.equal(tree.props.style.minHeight, 0);
  assert.equal(elements.filter(item => item.type === 'fieldset').length, 0);
  assert.ok(!JSON.stringify(tree).includes('Unused AVD'));
  const viewport = elements.find(item => Object.hasOwn(item.props || {}, 'data-mobile-preview-viewport'));
  assert.equal(viewport.props.style.flex, '1 1 0');
  assert.equal(viewport.props.style.minHeight, 0);
  const canvas = elements.find(item => item.type === 'canvas');
  assert.equal(canvas.props.style.position, 'absolute');
  assert.equal(canvas.props.style.width, '100%');
  assert.equal(canvas.props.style.height, '100%');
  assert.equal(canvas.props.style.objectFit, 'contain');
  assert.equal(canvas.props.width, undefined);
  const controls = elements.find(item => Object.hasOwn(item.props || {}, 'data-mobile-preview-controls'));
  assert.equal(controls.props.style.flexShrink, 0);
  const labels = nodes(controls).filter(item => item.type === 'button').map(item => item.props.children[0]);
  assert.deepEqual(labels, ['Home', 'Back', 'Pause']);
  const diagnostics = elements.find(item => item.type === 'details');
  assert.equal(diagnostics.props.open, undefined);
  assert.ok(!nodes(diagnostics).includes(controls));
  const ready = render('stopped');
  assert.ok(JSON.stringify(ready).includes(app.t('previewReady')));
  assert.ok(!JSON.stringify(ready).includes('Live preview'));
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

function previewHarness() {
  const raf = new Map(); let rafId = 0; let cleared = 0;
  const browser = {
    VideoDecoder: class {
      static async isConfigSupported(config) { return { supported: true, config }; }
      constructor(callbacks) { this.callbacks = callbacks; this.decodeQueueSize = 0; this.state = 'unconfigured'; }
      configure() { this.state = 'configured'; }
      decode() { this.callbacks.output({ close() {} }); }
      close() { this.state = 'closed'; }
    },
    EncodedVideoChunk: class { constructor(value) { Object.assign(this, value); } },
    requestAnimationFrame: fn => { const id = ++rafId; raf.set(id, fn); queueMicrotask(() => { if (raf.delete(id)) fn(); }); return id; },
    cancelAnimationFrame: id => raf.delete(id),
  };
  const app = harness({}, browser);
  const canvasEvents = new Map();
  const canvas = { getContext: () => ({ clearRect() { cleared++; }, drawImage() {} }),
    addEventListener: (name, fn) => canvasEvents.set(name, fn), removeEventListener: name => canvasEvents.delete(name),
    getBoundingClientRect: () => ({ left: 0, top: 0, width: 100, height: 200 }), focus() {},
    setPointerCapture() {}, hasPointerCapture: () => false,
  };
  const geometry = { width: 100, height: 200, display_width: 100, display_height: 200, rotation: 0 };
  let nextStream = 0, finish;
  app.setHandler(method => method === 'preview.start'
    ? { stream: `stream-${++nextStream}`, epoch: 70 + nextStream, generation: 1, geometry, capabilities: { video: true, input: true } } : undefined);
  const encode = (kind, bytes) => {
    const out = new Uint8Array(28 + bytes.length), view = new DataView(out.buffer);
    view.setUint32(0, 0x4d505031); out[4] = kind; view.setBigUint64(8, 1n); view.setUint32(16, bytes.length); out.set(bytes, 28); return out;
  };
  const config = new Uint8Array(21), view = new DataView(config.buffer);
  view.setUint32(0, 100); view.setUint32(4, 200);
  config.set([0, 0, 0, 1, 0x67, 0x42, 0xc0, 0x1e, 0, 0, 1, 0x68, 1], 8);
  const first = encode(0, config), key = encode(1, Uint8Array.of(0, 0, 1, 0x65, 1));
  const media = new Uint8Array(first.length + key.length); media.set(first); media.set(key, first.length);
  app.setMediaHandler(async () => {
    let initial = true;
    const reader = { async read() { if (initial) { initial = false; return { done: false, value: media }; } return new Promise(resolve => { finish = resolve; }); },
      async cancel() { finish?.({ done: true }); } };
    return { ok: true, headers: { get: () => 'application/octet-stream' }, body: { getReader: () => reader } };
  });
  return { app, canvas, canvasEvents, geometry, eof: () => finish?.({ done: true }), get cleared() { return cleared; } };
}

test('preview controller uses a distinct stream epoch; EOF resets input and retains the chat lease', async () => {
  const f = previewHarness(), ui = f.app.ui('chat-a');
  await ui.connect(DEVICE.id); await ui.startPreview(f.canvas); await flush();
  assert.equal(f.app.state().sessions['chat-a'].preview.status, 'live');
  assert.equal(f.app.requests.find(item => item.method === 'preview.start').timeoutMs, 47000);
  ui.pressKey(4); await flush();
  const inputs = f.app.requests.filter(item => item.method === 'input.send');
  assert.ok(inputs[0].params.requests.every(item => item.epoch === 71));
  assert.deepEqual(inputs[0].params.requests.map(item => item.command.event.phase), ['down', 'up']);
  f.eof(); await flush();
  assert.equal(f.app.state().sessions['chat-a'].preview.status, 'error');
  assert.equal(f.app.state().sessions['chat-a'].preview.error.key, 'streamEnded');
  assert.equal(f.app.state().sessions['chat-a'].binding, 'binding-chat-a');
  assert.equal(f.app.requests.filter(item => item.method === 'session.disconnect').length, 0);
  assert.equal(f.app.requests.findLast(item => item.method === 'input.send').params.requests[0].command.kind, 'reset');
  assert.equal(f.app.requests.find(item => item.method === 'preview.stop').timeoutMs, 47000);
  assert.equal(f.canvasEvents.size, 0); assert.ok(f.cleared > 0); f.app.dispose();
});

test('Home and Back handlers survive React reserved-prop filtering and submit distinct key edges', async () => {
  const f = previewHarness(), ui = f.app.ui('chat-a');
  await ui.connect(DEVICE.id); await ui.startPreview(f.canvas); await flush();
  const element = f.app.React.createElement(f.app.body.component, { ...ui, sessionId: 'chat-a',
    usePreview: selector => selector(f.app.state()),
    useTabInfo: () => ({ tab: { visible: true, signal: new AbortController().signal } }), t: f.app.t,
  });
  const tree = element.type(element.props);
  function buttons(value) {
    if (Array.isArray(value)) return value.flatMap(buttons);
    if (!value || typeof value !== 'object') return [];
    return [...(value.type === 'button' ? [value] : []), ...buttons(value.props?.children)];
  }
  const all = buttons(tree);
  const home = all.find(item => item.props.children[0] === 'Home');
  const back = all.find(item => item.props.children[0] === 'Back');
  assert.equal(home.props.disabled, false); assert.equal(back.props.disabled, false);
  assert.doesNotThrow(() => { home.props.onClick(); back.props.onClick(); });
  await flush();
  const commands = f.app.requests.filter(item => item.method === 'input.send').flatMap(item => item.params.requests);
  assert.deepEqual(commands.map(item => item.command.event.code), [3, 3, 4, 4]);
  assert.deepEqual(commands.map(item => item.command.event.phase), ['down', 'up', 'down', 'up']);
  assert.equal(Object.hasOwn(ui, 'key'), false);
  f.app.dispose();
});

test('registered canvas drag handlers submit normalized down, move and up events', async () => {
  const f = previewHarness(), ui = f.app.ui('chat-a');
  await ui.connect(DEVICE.id); await ui.startPreview(f.canvas); await flush();
  const event = y => ({ pointerId: 11, button: 0, isPrimary: true, clientX: 50, clientY: y, preventDefault() {} });
  f.canvasEvents.get('pointerdown')(event(50));
  f.canvasEvents.get('pointermove')(event(150));
  f.canvasEvents.get('pointerup')(event(150));
  await flush();
  const events = f.app.requests.filter(item => item.method === 'input.send').flatMap(item => item.params.requests.map(request => request.command.event));
  assert.deepEqual(events.map(item => item.phase), ['down', 'move', 'up']);
  assert.deepEqual(events.map(item => [item.x, item.y]), [[0.5, 0.25], [0.5, 0.75], [0.5, 0.75]]);
  assert.ok(events.every(item => item.width === 100 && item.height === 200));
  f.app.dispose();
});

test('panel close and page hiding stop capture without disconnecting the chat', async () => {
  const f = previewHarness(), ui = f.app.ui('chat-a');
  await ui.connect(DEVICE.id); await ui.startPreview(f.canvas); await flush();
  f.app.closeTab('chat-a'); await flush();
  assert.equal(f.app.state().sessions['chat-a'].preview.status, 'stopped');
  await ui.startPreview(f.canvas); await flush();
  f.app.document.visibilityState = 'hidden'; f.app.documentEvents.get('visibilitychange')(); await flush();
  assert.equal(f.app.state().sessions['chat-a'].preview.status, 'stopped');
  assert.equal(f.app.state().sessions['chat-a'].binding, 'binding-chat-a');
  assert.deepEqual(f.app.requests.filter(item => item.method === 'preview.stop').map(item => item.params.stream), ['stream-1', 'stream-2']);
  f.app.dispose();
});

test('backend unsupported errors remain explicit and do not erase the transport lease', async () => {
  const f = previewHarness(), ui = f.app.ui('chat-a');
  f.app.setHandler(method => { if (method === 'preview.start') throw { code: 'UNSUPPORTED', message: 'Configure capture artifacts', hint: 'See setup' }; });
  await ui.connect(DEVICE.id); await ui.startPreview(f.canvas);
  assert.equal(f.app.state().sessions['chat-a'].preview.error.code, 'UNSUPPORTED');
  assert.equal(f.app.state().sessions['chat-a'].binding, 'binding-chat-a');
  assert.equal(f.app.requests.filter(item => item.url.endsWith('/media')).length, 0);
  f.app.dispose();
});

test('first-frame deadline never leaves a non-decoding connection labeled live', async () => {
  const f = previewHarness(), ui = f.app.ui('chat-a');
  f.app.setMediaHandler(async () => ({ ok: true, headers: { get: () => 'application/octet-stream' },
    body: { getReader: () => ({ read: () => new Promise(() => {}), async cancel() {} }) } }));
  await ui.connect(DEVICE.id); await ui.startPreview(f.canvas); await flush();
  assert.equal(f.app.state().sessions['chat-a'].preview.status, 'buffering');
  const timeout = [...f.app.timeouts.values()].find(item => item.ms === 10000); assert.ok(timeout);
  timeout.fn(); await flush();
  assert.equal(f.app.state().sessions['chat-a'].preview.status, 'error');
  assert.equal(f.app.state().sessions['chat-a'].binding, 'binding-chat-a');
  f.app.dispose();
});

test('a late obsolete start is stopped before a replacement preview may start', async () => {
  const f = previewHarness(), ui = f.app.ui('chat-a'); let finishOld; let count = 0;
  const descriptor = stream => ({ stream, epoch: stream === 'old-stream' ? 91 : 92, generation: 1,
    geometry: f.geometry, capabilities: { video: true, input: true } });
  f.app.setHandler(method => {
    if (method !== 'preview.start') return undefined;
    if (++count === 1) return new Promise(resolve => { finishOld = resolve; });
    return descriptor('new-stream');
  });
  await ui.connect(DEVICE.id);
  const old = ui.startPreview(f.canvas); await flush();
  const stopping = ui.stopPreview();
  const replacement = ui.startPreview(f.canvas); await flush();
  assert.equal(count, 1);
  finishOld(descriptor('old-stream')); await Promise.all([old, stopping, replacement]); await flush();
  assert.equal(f.app.state().sessions['chat-a'].preview.status, 'live');
  assert.deepEqual(f.app.requests.filter(item => item.method === 'preview.stop').map(item => item.params.stream), ['old-stream']);
  f.app.dispose();
});

for (const mountFirst of [false, true]) {
  test(`connection automatically previews with a ${mountFirst ? 'previously' : 'subsequently'} mounted canvas`, async () => {
    const f = previewHarness(), ui = f.app.ui('chat-a'), signal = new AbortController();
    let dispose;
    if (mountFirst) dispose = ui.mountPreview(f.canvas, signal.signal);
    await ui.connect(DEVICE.id); await flush();
    assert.equal(f.app.state().sessions['chat-a'].previewWanted, true);
    if (!mountFirst) {
      assert.equal(f.app.requests.filter(item => item.method === 'preview.start').length, 0);
      dispose = ui.mountPreview(f.canvas, signal.signal); await flush();
    }
    assert.equal(f.app.state().sessions['chat-a'].preview.status, 'live');
    assert.equal(ui.mountPreview(f.canvas, signal.signal), dispose);
    await f.app.tick(); await ui.activate(); await flush();
    assert.equal(f.app.requests.filter(item => item.method === 'preview.start').length, 1);
    dispose(); await flush();
    assert.equal(f.app.state().sessions['chat-a'].previewWanted, true);
    await ui.resumePreview();
    assert.equal(f.app.requests.filter(item => item.method === 'preview.start').length, 1);
    ui.mountPreview(f.canvas, signal.signal); await flush();
    assert.equal(f.app.requests.filter(item => item.method === 'preview.start').length, 2);
    f.app.dispose();
  });
}

test('document and page visibility suspend and automatically resume the current wanted view', async () => {
  const f = previewHarness(), ui = f.app.ui('chat-a');
  ui.mountPreview(f.canvas, new AbortController().signal);
  await ui.connect(DEVICE.id); await flush();
  f.app.document.visibilityState = 'hidden'; f.app.documentEvents.get('visibilitychange')(); await flush();
  assert.equal(f.app.state().sessions['chat-a'].preview.status, 'stopped');
  assert.equal(f.app.state().sessions['chat-a'].previewWanted, true);
  f.app.document.visibilityState = 'visible'; f.app.documentEvents.get('visibilitychange')(); await flush();
  assert.equal(f.app.state().sessions['chat-a'].preview.status, 'live');
  f.app.windowEvents.get('pagehide')(); await flush();
  assert.equal(f.app.state().sessions['chat-a'].preview.status, 'stopped');
  f.app.windowEvents.get('pageshow')(); await flush();
  assert.equal(f.app.state().sessions['chat-a'].preview.status, 'live');
  assert.equal(f.app.requests.filter(item => item.method === 'preview.start').length, 3);
  assert.equal(f.app.requests.filter(item => item.method === 'session.disconnect').length, 0);
  f.app.dispose();
});

test('explicit pause survives heartbeat, remount and background return until Resume or a new connection', async () => {
  const f = previewHarness(), ui = f.app.ui('chat-a');
  const dispose = ui.mountPreview(f.canvas, new AbortController().signal);
  await ui.connect(DEVICE.id); await flush(); await ui.pausePreview();
  assert.equal(f.app.state().sessions['chat-a'].previewWanted, false);
  await f.app.tick(); await ui.activate();
  dispose(); ui.mountPreview(f.canvas, new AbortController().signal); await flush();
  f.app.document.visibilityState = 'hidden'; f.app.documentEvents.get('visibilitychange')(); await flush();
  f.app.document.visibilityState = 'visible'; f.app.documentEvents.get('visibilitychange')(); await flush();
  assert.equal(f.app.state().sessions['chat-a'].previewWanted, false);
  assert.equal(f.app.requests.filter(item => item.method === 'preview.start').length, 1);
  await ui.resumePreview(); await flush();
  assert.equal(f.app.state().sessions['chat-a'].preview.status, 'live');
  await ui.pausePreview(); await ui.disconnect(); await ui.connect(DEVICE.id); await flush();
  assert.equal(f.app.state().sessions['chat-a'].previewWanted, true);
  assert.equal(f.app.requests.filter(item => item.method === 'preview.start').length, 3);
  f.app.dispose();
});

test('rapid hide/show waits for both an obsolete start receipt and its pending stop before restarting', async () => {
  const f = previewHarness(), ui = f.app.ui('chat-a'); let finishStart, finishStop, starts = 0;
  const descriptor = stream => ({ stream, epoch: ++starts, generation: 1, geometry: f.geometry, capabilities: { video: true, input: true } });
  f.app.setHandler((method, params) => {
    if (method === 'preview.start') {
      const result = descriptor(starts === 0 ? 'old' : 'new');
      return starts === 1 ? new Promise(resolve => { finishStart = () => resolve(result); }) : result;
    }
    if (method === 'preview.stop' && params.stream === 'old') return new Promise(resolve => { finishStop = () => resolve({ stopped: true }); });
    return undefined;
  });
  ui.mountPreview(f.canvas, new AbortController().signal); await ui.connect(DEVICE.id); await flush();
  f.app.document.visibilityState = 'hidden'; f.app.documentEvents.get('visibilitychange')();
  f.app.document.visibilityState = 'visible'; f.app.documentEvents.get('visibilitychange')(); await flush();
  assert.equal(starts, 1);
  assert.equal(f.app.requests.find(item => item.method === 'preview.start').options.signal.aborted, false);
  finishStart(); await flush();
  assert.equal(typeof finishStop, 'function'); assert.equal(starts, 1);
  assert.notEqual(f.app.state().sessions['chat-a'].preview.status, 'live');
  f.app.document.visibilityState = 'hidden'; f.app.documentEvents.get('visibilitychange')();
  f.app.document.visibilityState = 'visible'; f.app.documentEvents.get('visibilitychange')(); await flush();
  assert.equal(starts, 1); finishStop(); await flush();
  assert.equal(starts, 2); assert.equal(f.app.state().sessions['chat-a'].preview.status, 'live');
  assert.deepEqual(f.app.requests.filter(item => item.url.endsWith('/media')).map(item => item.media.stream), ['new']);
  f.app.dispose();
});

test('replacement canvases own their disposer and an obsolete view cannot stop the new preview', async () => {
  const f = previewHarness(), ui = f.app.ui('chat-a'), firstSignal = new AbortController(), secondSignal = new AbortController();
  const firstDispose = ui.mountPreview(f.canvas, firstSignal.signal);
  await ui.connect(DEVICE.id); await flush();
  const nextCanvas = { ...f.canvas };
  ui.mountPreview(nextCanvas, secondSignal.signal); await flush();
  firstDispose(); firstSignal.abort(); await flush();
  assert.equal(f.app.requests.filter(item => item.method === 'preview.start').length, 2);
  assert.equal(f.app.state().sessions['chat-a'].preview.status, 'live');
  secondSignal.abort(); await flush();
  assert.equal(f.app.state().sessions['chat-a'].preview.status, 'stopped');
  assert.equal(f.app.state().sessions['chat-a'].previewWanted, true); f.app.dispose();
});

for (const ending of ['abort', 'disconnect', 'unload']) {
  test(`${ending} during a pending start prevents queued capture from appearing later`, async () => {
    const f = previewHarness(), ui = f.app.ui('chat-a'), signal = new AbortController(); let finish;
    f.app.setHandler(method => method === 'preview.start' ? new Promise(resolve => { finish = () => resolve({
      stream: 'late', epoch: 1, generation: 1, geometry: f.geometry, capabilities: { video: true, input: true },
    }); }) : undefined);
    ui.mountPreview(f.canvas, signal.signal); await ui.connect(DEVICE.id); await flush();
    let completion;
    if (ending === 'abort') signal.abort();
    if (ending === 'disconnect') completion = ui.disconnect();
    if (ending === 'unload') f.app.dispose();
    finish(); await completion; await flush();
    assert.equal(f.app.requests.filter(item => item.method === 'preview.start').length, 1);
    assert.equal(f.app.requests.filter(item => item.url.endsWith('/media')).length, 0);
    if (ending === 'disconnect') assert.equal(f.app.state().sessions['chat-a'].binding, null);
    if (ending === 'abort') { await ui.resumePreview(); assert.equal(f.app.requests.filter(item => item.method === 'preview.start').length, 1); }
    f.app.dispose();
  });
}

test('unsupported preview failures latch without hot-looping and only explicit or visibility intent retries', async () => {
  const f = previewHarness(), ui = f.app.ui('chat-a'), signal = new AbortController(); let starts = 0;
  f.app.setHandler(method => {
    if (method === 'preview.start') { starts++; throw { code: 'UNSUPPORTED', message: 'Missing capture backend' }; }
  });
  ui.mountPreview(f.canvas, signal.signal); await ui.connect(DEVICE.id); await flush();
  assert.equal(starts, 1); assert.equal(f.app.state().sessions['chat-a'].preview.error.code, 'UNSUPPORTED');
  await f.app.tick(); await ui.activate(); ui.mountPreview(f.canvas, signal.signal); await flush();
  assert.equal(starts, 1);
  await ui.resumePreview(); await flush(); assert.equal(starts, 2);
  f.app.document.visibilityState = 'hidden'; f.app.documentEvents.get('visibilitychange')(); await flush();
  f.app.document.visibilityState = 'visible'; f.app.documentEvents.get('visibilitychange')(); await flush();
  assert.equal(starts, 3);
  assert.equal(f.app.state().sessions['chat-a'].previewWanted, true); f.app.dispose();
});

test('input failure releases the preview without automatically replaying or retrying input', async () => {
  const f = previewHarness(), ui = f.app.ui('chat-a');
  ui.mountPreview(f.canvas, new AbortController().signal); await ui.connect(DEVICE.id); await flush();
  f.app.setHandler((method, params) => method === 'input.send'
    ? { replies: params.requests.map(item => ({ seq: item.seq, ok: false, code: 'INPUT_FAILED' })) } : undefined);
  ui.pressKey(4); await flush(); await f.app.tick(); await flush();
  assert.equal(f.app.state().sessions['chat-a'].preview.error.key, 'inputFailed');
  assert.equal(f.app.requests.filter(item => item.method === 'preview.start').length, 1);
  assert.equal(f.app.requests.filter(item => item.method === 'preview.stop').length, 1);
  const commands = f.app.requests.filter(item => item.method === 'input.send').flatMap(item => item.params.requests);
  assert.deepEqual(commands.filter(item => item.command.kind === 'input').map(item => item.command.event.phase), ['down', 'up']);
  assert.ok(commands.some(item => item.command.kind === 'reset')); f.app.dispose();
});

test('quiescing immediately clears Live while a slow stop retains geometry and serializes Resume', async () => {
  const f = previewHarness(), ui = f.app.ui('chat-a'); let finishStop;
  ui.mountPreview(f.canvas, new AbortController().signal); await ui.connect(DEVICE.id); await flush();
  const oldHandler = (method, params) => method === 'preview.stop' && params.stream === 'stream-1'
    ? new Promise(resolve => { finishStop = () => resolve({ stopped: true }); }) : method === 'preview.start'
      ? { stream: 'stream-2', epoch: 72, generation: 1, geometry: f.geometry, capabilities: { video: true, input: true } } : undefined;
  f.app.setHandler(oldHandler);
  const pausing = ui.pausePreview();
  assert.notEqual(f.app.state().sessions['chat-a'].preview.status, 'live');
  assert.deepEqual(f.app.state().sessions['chat-a'].preview.geometry, f.geometry);
  await flush(); const resuming = ui.resumePreview(); await flush();
  assert.equal(f.app.requests.filter(item => item.method === 'preview.start').length, 1);
  assert.notEqual(f.app.state().sessions['chat-a'].preview.status, 'live');
  finishStop(); await Promise.all([pausing, resuming]); await flush();
  assert.equal(f.app.state().sessions['chat-a'].preview.status, 'live'); f.app.dispose();
});

test('even explicit start waits for visibility before creating capture', async () => {
  const f = previewHarness(), ui = f.app.ui('chat-a');
  f.app.document.visibilityState = 'hidden'; f.app.documentEvents.get('visibilitychange')();
  await ui.connect(DEVICE.id); await ui.startPreview(f.canvas); await flush();
  assert.equal(f.app.requests.filter(item => item.method === 'preview.start').length, 0);
  f.app.document.visibilityState = 'visible'; f.app.documentEvents.get('visibilitychange')(); await flush();
  assert.equal(f.app.requests.filter(item => item.method === 'preview.start').length, 1);
  assert.equal(f.app.state().sessions['chat-a'].preview.status, 'live'); f.app.dispose();
});

function widthFitHarness({ patched = true } = {}) {
  let frameId = 0, savedWidth = 540, currentWidth = savedWidth, active = null;
  const frames = new Map(), observers = [], claims = [], updates = [];
  const layout = patched ? { requestRightbarWidth(width) {
    const claim = { width, disposed: false };
    claims.push(claim); active = claim; currentWidth = Math.max(300, Math.min(1000, width));
    return {
      update(next) { updates.push(next); if (active === claim) currentWidth = Math.max(300, Math.min(1000, next)); },
      dispose() { claim.disposed = true; if (active === claim) { active = null; currentWidth = savedWidth; } },
    };
  } } : {};
  const browser = {
    ResizeObserver: class {
      constructor(callback) { this.callback = callback; this.nodes = new Set(); observers.push(this); }
      observe(node) { this.nodes.add(node); }
      disconnect() { this.nodes.clear(); }
    },
    requestAnimationFrame: callback => { const id = ++frameId; frames.set(id, callback); return id; },
    cancelAnimationFrame: id => frames.delete(id),
    getComputedStyle: node => node.padding,
  };
  const app = harness({}, browser, { layout });
  const section = { height: 800, padding: { paddingLeft: '12px', paddingRight: '12px', paddingTop: '12px', paddingBottom: '12px' },
    getBoundingClientRect() { return { width: currentWidth, height: this.height }; } };
  const viewport = { height: 640, getBoundingClientRect() { return { width: currentWidth - 24, height: this.height }; } };
  return { app, section, viewport, frames, observers, claims, updates,
    begin: config => app.ui('chat-a').beginWidthFit({ section, viewport, geometry: { width: 576, height: 1280 }, ...config }),
    flush() { const callbacks = [...frames.values()]; frames.clear(); for (const callback of callbacks) callback(); },
    resize() { for (const observer of observers) if (observer.nodes.size) observer.callback([]); },
    manual(width) { savedWidth = width; currentWidth = width; active = null; },
    get currentWidth() { return currentWidth; }, get savedWidth() { return savedWidth; },
  };
}

test('width fitting predicts one phone from own viewport height, batches observation and updates geometry', () => {
  const f = widthFitHarness(), fit = f.begin();
  assert.equal(f.frames.size, 1);
  f.resize(); f.resize(); assert.equal(f.frames.size, 1);
  f.flush();
  assert.equal(f.claims[0].width, 326);
  assert.equal(f.currentWidth, 326); assert.equal(f.savedWidth, 540);
  assert.deepEqual([...f.observers[0].nodes], [f.section, f.viewport]);
  f.resize(); f.flush(); assert.equal(f.updates.length, 0);
  f.viewport.height = 720; f.resize(); f.resize(); f.flush();
  assert.deepEqual(f.updates, [362]);
  fit.update({ geometry: { width: 720, height: 1280 } }); f.flush();
  assert.deepEqual(f.updates, [362, 439]); assert.equal(f.claims.length, 1);
  fit.dispose(); assert.equal(f.currentWidth, 540); f.app.dispose();
});

test('picker fitting uses section content height and portrait fallback without reacquiring on connection', () => {
  const f = widthFitHarness();
  f.section.padding = { paddingLeft: '16px', paddingRight: '16px', paddingTop: '16px', paddingBottom: '16px' };
  const fit = f.begin({ viewport: null, geometry: null }); f.flush();
  assert.equal(f.claims[0].width, 378);
  assert.deepEqual([...f.observers[0].nodes], [f.section]);
  f.section.padding = { paddingLeft: '12px', paddingRight: '12px', paddingTop: '12px', paddingBottom: '12px' };
  fit.update({ viewport: f.viewport, geometry: { width: 576, height: 1280 } }); f.flush();
  assert.equal(f.claims.length, 1); assert.deepEqual(f.updates, [326]);
  assert.deepEqual([...f.observers[0].nodes], [f.section, f.viewport]);
  fit.dispose(); f.app.dispose();
});

test('manual width overrides survive later height and geometry changes without a new request', () => {
  const f = widthFitHarness(), fit = f.begin(); f.flush();
  f.manual(650);
  f.viewport.height = 900; f.resize(); f.flush();
  fit.update({ geometry: { width: 1280, height: 576 } }); f.flush();
  assert.equal(f.claims.length, 1); assert.equal(f.updates.length, 2);
  assert.equal(f.currentWidth, 650); assert.equal(f.savedWidth, 650);
  fit.dispose(); assert.equal(f.currentWidth, 650); f.app.dispose();
});

test('width-fit cleanup cancels observer work and aborts safely without clearing a newer host claim', () => {
  const f = widthFitHarness(), signal = new AbortController();
  const old = f.begin({ signal: signal.signal }); f.flush();
  const next = f.begin({ geometry: { width: 1, height: 2 } }); f.flush();
  assert.equal(f.currentWidth, 356);
  f.resize(); signal.abort();
  assert.equal(f.observers[0].nodes.size, 0); assert.equal(f.claims[0].disposed, true);
  f.flush(); assert.equal(f.currentWidth, 356);
  old.update({ geometry: { width: 99, height: 1 } }); old.dispose(); f.resize();
  next.dispose(); assert.equal(f.frames.size, 0); assert.equal(f.currentWidth, 540);
  assert.ok(f.observers.every(observer => observer.nodes.size === 0)); f.app.dispose();
});

test('zero-height and old-host width fitting are inert until an eligible measurement exists', () => {
  const f = widthFitHarness(); f.viewport.height = 0;
  const fit = f.begin(); f.flush(); assert.equal(f.claims.length, 0);
  f.viewport.height = 640; f.resize(); f.flush(); assert.equal(f.claims.length, 1);
  fit.dispose(); f.app.dispose();
  const old = widthFitHarness({ patched: false }), fallback = old.begin();
  fallback.update({ geometry: { width: 1, height: 2 } }); fallback.dispose();
  assert.equal(old.observers.length, 0); assert.equal(old.frames.size, 0); assert.equal(old.currentWidth, 540); old.app.dispose();
});

function panelWidthLifecycle(f) {
  const refs = [], effects = []; let refIndex = 0, effectIndex = 0, pendingEffects = [], mountedRefs = new Set(), stopCalls = 0;
  const signal = new AbortController();
  const activate = () => {}, mountPreview = () => () => { stopCalls++; };
  f.app.React.useRef = initial => refs[refIndex++] ?? (refs[refIndex - 1] = { current: initial });
  f.app.React.useEffect = (create, dependencies) => {
    const index = effectIndex++, previous = effects[index];
    if (!previous || dependencies.some((value, i) => value !== previous.dependencies[i])) pendingEffects.push({ index, create, dependencies });
  };
  function nodes(value) {
    if (Array.isArray(value)) return value.flatMap(nodes);
    if (!value || typeof value !== 'object') return [];
    return [value, ...nodes(value.props?.children)];
  }
  return {
    signal,
    render({ visible = true, fullscreen = false, sole = true, sessionId = 'chat-a', connected = true, geometry = { width: 576, height: 1280 } } = {}) {
      refIndex = 0; effectIndex = 0; pendingEffects = [];
      const state = { ...f.app.state(), sessions: connected ? { [sessionId]: {
        binding: 'binding', session: { device: DEVICE }, verified: true, busy: null, preview: { status: 'live', geometry },
      } } : {} };
      const tree = f.app.body.component({ ...f.app.ui(sessionId), sessionId, activate, mountPreview,
        usePreview: selector => selector(state),
        useTabInfo: () => ({ tab: { visible, signal: signal.signal }, sidebar: { fullscreen }, panel: sole === null ? {} : { isSoleDockedPane: sole } }), t: f.app.t,
      });
      const elements = nodes(tree), nextRefs = new Set(elements.flatMap(node => node.props?.ref ? [node.props.ref] : []));
      for (const ref of mountedRefs) if (!nextRefs.has(ref)) ref.current = null;
      for (const node of elements) if (node.props?.ref) {
        node.props.ref.current = node.type === 'section' ? f.section
          : Object.hasOwn(node.props, 'data-mobile-preview-viewport') ? f.viewport : { width: 576, height: 1280 };
      }
      mountedRefs = nextRefs;
      for (const { index } of pendingEffects) effects[index]?.cleanup?.();
      for (const { index, create, dependencies } of pendingEffects) effects[index] = { dependencies, cleanup: create() };
      return tree;
    },
    unmount() { for (const effect of effects) effect?.cleanup?.(); },
    get stopCalls() { return stopCalls; },
  };
}

test('Panel claims width only for a visible sole docked pane and releases for fullscreen, split, hidden and unmount', () => {
  const f = widthFitHarness(), panel = panelWidthLifecycle(f);
  panel.render(); f.flush(); assert.equal(f.claims.length, 1);
  panel.render({ geometry: { width: 720, height: 1280 } }); f.flush();
  f.viewport.height = 700; f.resize(); f.flush();
  assert.equal(f.claims.length, 1); assert.equal(panel.stopCalls, 0);
  panel.render({ fullscreen: true }); assert.equal(f.claims[0].disposed, true); assert.equal(panel.stopCalls, 0);
  panel.render(); f.flush(); assert.equal(f.claims.length, 2);
  panel.render({ sole: false }); assert.equal(f.claims[1].disposed, true); assert.equal(panel.stopCalls, 0);
  panel.render(); f.flush(); assert.equal(f.claims.length, 3);
  panel.render({ visible: false }); assert.equal(f.claims[2].disposed, true);
  panel.render(); f.flush(); assert.equal(f.claims.length, 4);
  panel.unmount(); assert.equal(f.claims[3].disposed, true); assert.equal(f.frames.size, 0); f.app.dispose();
});

test('Panel never claims missing-field, fullscreen, hidden, split or floating layout states', () => {
  for (const options of [{ sole: null }, { fullscreen: true }, { visible: false }, { sole: false }]) {
    const f = widthFitHarness(), panel = panelWidthLifecycle(f);
    panel.render(options); f.flush();
    assert.equal(f.claims.length, 0); assert.equal(f.currentWidth, 540);
    panel.unmount(); f.app.dispose();
  }
});

test('Panel connection and geometry transitions keep one handle, while a new session may make a fresh claim', () => {
  const f = widthFitHarness(), panel = panelWidthLifecycle(f);
  panel.render({ connected: false }); f.flush();
  assert.equal(f.claims.length, 1);
  f.manual(660);
  panel.render({ connected: true }); f.flush();
  panel.render({ geometry: { width: 1280, height: 576 } }); f.flush();
  assert.equal(f.claims.length, 1); assert.equal(f.currentWidth, 660);
  panel.render({ sessionId: 'chat-b' }); f.flush(); assert.equal(f.claims.length, 2);
  panel.signal.abort(); assert.equal(f.claims[1].disposed, true); assert.equal(f.frames.size, 0);
  panel.unmount(); f.app.dispose();
});

test('plugin declares the host-provided layout service and client module', async () => {
  const app = harness();
  assert.ok(app.injectedServices.includes('layout'));
  const manifest = JSON.parse(await readFile(new URL('../package.json', import.meta.url), 'utf8'));
  assert.ok(manifest.dsh.client.inject.includes('@deepseek-ai/dsh-client-ui-layout'));
  assert.equal(manifest.dependencies, undefined); app.dispose();
});
