import assert from 'node:assert/strict';
import test from 'node:test';
import { registerAgentTool } from '../src/host/agent.mjs';

function fixture(requestPlatform) {
  let injection;
  const tools = new Map();
  const cleanup = [];
  registerAgentTool({ inject(dependencies, apply) { injection = { dependencies, apply }; } }, { requestPlatform });
  const mount = () => injection.apply({ tools: { register(definition) {
    assert.equal(tools.has(definition.name), false);
    tools.set(definition.name, definition);
    const dispose = () => tools.delete(definition.name);
    cleanup.push(dispose);
    return dispose;
  } } });
  return {
    injection, tools, mount,
    unmount() { for (const dispose of cleanup.splice(0)) dispose(); },
    tool() { return tools.get('open_mobile_preview'); },
  };
}

const execution = (sessionId = 'chat-a', signal = new AbortController().signal) => ({
  agent: { session: { id: sessionId } }, signal,
});
const state = (sessionId, platform) => ({
  sessionId, platform, source: 'agent', revision: 1, epoch: 'host-epoch', available: platform === 'android',
});

test('one tool waits for the optional registry and its registration belongs to that registry lifecycle', () => {
  const f = fixture(() => assert.fail('Registration must not select a platform.'));
  assert.deepEqual(f.injection.dependencies, ['tools']);
  assert.equal(f.tools.size, 0);
  f.mount();
  assert.deepEqual([...f.tools.keys()], ['open_mobile_preview']);
  f.unmount();
  assert.equal(f.tools.size, 0);
  f.mount();
  assert.deepEqual([...f.tools.keys()], ['open_mobile_preview']);
  f.unmount();
});

test('tool advertises only a platform enum and declares its full canonical result', () => {
  const f = fixture(() => {});
  f.mount();
  assert.deepEqual(f.tool().parameters, {
    type: 'object', properties: { platform: { type: 'string', enum: ['android', 'ios'] } },
    required: ['platform'], additionalProperties: false,
  });
  assert.deepEqual(f.tool().output.schema, {
    type: 'object',
    properties: {
      sessionId: { type: 'string' }, platform: { type: 'string', enum: ['android', 'ios'] },
      source: { type: 'string', const: 'agent' }, revision: { type: 'integer' },
      epoch: { type: 'string' }, available: { type: 'boolean' },
    },
    required: ['sessionId', 'platform', 'source', 'revision', 'epoch', 'available'],
    additionalProperties: false,
  });
});

test('Android and iOS requests use only the calling conversation and forward its cancellation', async () => {
  const calls = [];
  const f = fixture(async (sessionId, platform, options) => {
    calls.push({ sessionId, platform, options });
    return state(sessionId, platform);
  });
  f.mount();
  for (const [sessionId, platform] of [['chat-a', 'android'], ['chat-b', 'ios']]) {
    const exec = execution(sessionId);
    exec.sessionId = 'unrelated-chat';
    exec.agent.id = 'unrelated-agent';
    const result = await f.tool().execute({ platform }, exec);
    assert.deepEqual(result, state(sessionId, platform));
    assert.deepEqual(calls.at(-1), { sessionId, platform, options: { signal: exec.signal } });
    const [{ text }] = f.tool().output.render({ platform }, result);
    assert.ok(!text.includes(sessionId));
    if (platform === 'ios') {
      assert.match(text, /iOS backend is unavailable/);
      assert.match(text, /platform selection only/);
    } else {
      assert.match(text, /Choose a device in the panel to connect/);
    }
    assert.doesNotMatch(text, /started|booted|connected|live video|vision/i);
  }
  assert.equal(calls.length, 2);
});

test('invalid arguments and injected conversation ids fail before any platform mutation without echoing input', async () => {
  const f = fixture(() => assert.fail('Invalid input reached the service.'));
  f.mount();
  for (const args of [null, [], 'private-value', 1, {}, { platform: null }, { platform: 'private-value' },
    { platform: 'Android' }, { platform: ['android'] }, { platform: 'android', sessionId: 'private-value' },
    { platform: 'ios', device: 'private-value' }, Object.create({ platform: 'android' })]) {
    await assert.rejects(f.tool().execute(args, execution()), error => {
      assert.equal(error.code, 'INVALID_ARGUMENT');
      assert.ok(!`${error.message} ${error.hint}`.includes('private-value'));
      return true;
    });
  }
});

test('a missing calling conversation fails before platform mutation', async () => {
  const f = fixture(() => assert.fail('An unowned request reached the service.'));
  f.mount();
  for (const exec of [undefined, {}, { agent: null }, { agent: { id: 'private-value' } }, execution('')]) {
    await assert.rejects(f.tool().execute({ platform: 'android' }, exec), error => {
      assert.equal(error.code, 'SESSION_REQUIRED');
      assert.ok(!error.message.includes('private-value'));
      return true;
    });
  }
});

test('pre-cancelled requests cannot mutate selection or echo the cancellation reason', async () => {
  const f = fixture(() => assert.fail('Cancelled input reached the service.'));
  f.mount();
  const controller = new AbortController();
  controller.abort(new Error('private-value'));
  await assert.rejects(f.tool().execute({ platform: 'android' }, execution('chat-a', controller.signal)), error => {
    assert.equal(error.code, 'ABORTED');
    assert.ok(!error.message.includes('private-value'));
    return true;
  });
});

test('in-flight cancellation reaches the service and returns a non-echoing failure', async () => {
  const controller = new AbortController();
  const f = fixture(async (_sessionId, _platform, { signal }) => {
    assert.equal(signal, controller.signal);
    return new Promise((_resolve, reject) => signal.addEventListener('abort', () => reject(signal.reason), { once: true }));
  });
  f.mount();
  const pending = f.tool().execute({ platform: 'android' }, execution('chat-a', controller.signal));
  controller.abort(new Error('private-value'));
  await assert.rejects(pending, error => error.code === 'ABORTED' && !error.message.includes('private-value'));
});

test('connection conflicts remain service failures instead of successful selections', async () => {
  const busy = Object.assign(new Error('Disconnect the current device before changing platform.'), { code: 'BUSY' });
  const f = fixture(async () => { throw busy; });
  f.mount();
  await assert.rejects(f.tool().execute({ platform: 'ios' }, execution()), error => error === busy);
});

test('available iOS tool result describes capability-dependent Simulator controls without claiming a connection', async () => {
  const f = fixture(async (sessionId, platform) => ({ ...state(sessionId, platform), available: true }));
  f.mount();
  const value = await f.tool().execute({ platform: 'ios' }, execution());
  const [{ text }] = f.tool().output.render({ platform: 'ios' }, value);
  assert.match(text, /Choose a simulator/);
  assert.match(text, /supported simulators offer touch, drag and Home/);
  assert.match(text, /read-only fallback when input is unavailable/);
  assert.doesNotMatch(text, /backend is unavailable|platform selection only|started|booted|connected/);
  assert.match(f.tool().description, /touch and Home when available/);
});
