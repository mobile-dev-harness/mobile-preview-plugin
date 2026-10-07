import assert from 'node:assert/strict';
import { readFile } from 'node:fs/promises';
import test from 'node:test';
import vm from 'node:vm';

const artifact = (await readFile(new URL('../client.js', import.meta.url), 'utf8'))
  .replace("return { inject: ['slots'", "return { deviceFrameSize, bindCanvasInput, inject: ['slots'");

function target() {
  const listeners = new Map();
  return {
    addEventListener: (name, callback) => listeners.set(name, callback),
    removeEventListener: name => listeners.delete(name),
    dispatch: (name, fields) => listeners.get(name)?.({ preventDefault() {}, ...fields }),
  };
}

function harness() {
  let api;
  const window = { ...target(), __ModuleLoader__: { load(definition) { api = definition.factory(() => ({})); } } };
  vm.runInNewContext(artifact, { window }, { filename: 'client.js' });
  return api;
}

test('portrait frame fits its bounds and preserves the inner screen ratio', () => {
  const { deviceFrameSize } = harness();
  const size = deviceFrameSize({ width: 340, height: 700 }, { width: 576, height: 1280 });
  assert.ok(size.width + 8 <= 340);
  assert.equal(size.height + 8, 700);
  assert.ok(Math.abs((size.width - 20) / (size.height - 24) - 0.45) < 1e-12);
});

test('landscape frame is limited by width without cropping or stretching the screen', () => {
  const { deviceFrameSize } = harness();
  const size = deviceFrameSize({ width: 500, height: 800 }, { width: 1280, height: 720 });
  assert.equal(size.width + 8, 500);
  assert.ok(size.height + 8 <= 800);
  assert.ok(Math.abs((size.width - 20) / (size.height - 24) - 1280 / 720) < 1e-12);
});

test('unknown geometry uses a portrait frame and unusable bounds remain hidden', () => {
  const { deviceFrameSize } = harness();
  assert.deepEqual(deviceFrameSize({ width: 320, height: 640 }, null),
    deviceFrameSize({ width: 320, height: 640 }, { width: 9, height: 20 }));
  for (const bounds of [{ width: 20, height: 640 }, { width: 320, height: 24 }, { width: NaN, height: 640 }, { width: 320, height: Infinity }]) {
    assert.equal(deviceFrameSize(bounds, null), null);
  }
});

test('touch coordinates use the inset screen and reject presses in the device bezel', () => {
  const { deviceFrameSize, bindCanvasInput } = harness();
  const geometry = { width: 576, height: 1280 };
  const frame = deviceFrameSize({ width: 320, height: 640 }, geometry);
  const screen = { left: 44, top: 56, width: frame.width - 20, height: frame.height - 24 };
  const events = [], canvas = { ...target(), getBoundingClientRect: () => screen,
    focus() {}, setPointerCapture() {}, hasPointerCapture: () => false };
  const detach = bindCanvasInput(canvas, geometry, { enqueue: event => events.push(event), reset() {} }, () => true);
  const pointer = { button: 0, pointerId: 1, isPrimary: true };
  canvas.dispatch('pointerdown', { ...pointer, clientX: screen.left - 4, clientY: screen.top + screen.height / 2 });
  assert.equal(events.length, 0);
  canvas.dispatch('pointerdown', { ...pointer, clientX: screen.left + screen.width / 2, clientY: screen.top + screen.height / 2 });
  canvas.dispatch('pointerup', { ...pointer, clientX: screen.left + screen.width / 2, clientY: screen.top + screen.height / 2 });
  assert.deepEqual(events.map(({ event }) => [event.phase, event.x, event.y]), [['down', 0.5, 0.5], ['up', 0.5, 0.5]]);
  detach();
});
