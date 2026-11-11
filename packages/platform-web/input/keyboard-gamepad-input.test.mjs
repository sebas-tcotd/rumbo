import test from 'node:test';
import assert from 'node:assert/strict';
import { createKeyboardGamepadInput } from './keyboard-gamepad-input.ts';

function fakeWindow(pads = []) {
  const target = new EventTarget();
  target.navigator = { getGamepads: () => pads };
  return target;
}
function key(type, code, extra = {}) {
  const e = new Event(type, { cancelable: true });
  Object.assign(e, { code, repeat: false, ...extra.props });
  if (extra.target) Object.defineProperty(e, 'target', { value: extra.target });
  return e;
}
const pad = (axis, buttons) => ({
  connected: true,
  mapping: 'standard',
  axes: [axis],
  buttons: Array.from({ length: 12 }, (_, i) => ({ pressed: !!buttons[i], value: buttons[i] ?? 0 })),
});

test('keyboard keys map to a digital frame and fire actions once per press', () => {
  const win = fakeWindow();
  const actions = [];
  const input = createKeyboardGamepadInput(win, (code) => actions.push(code));
  const arrow = key('keydown', 'ArrowUp');
  win.dispatchEvent(arrow);
  assert.equal(arrow.defaultPrevented, true, 'arrows must not scroll the page');
  win.dispatchEvent(key('keydown', 'KeyA'));
  win.dispatchEvent(key('keydown', 'KeyA', { props: { repeat: true } }));
  win.dispatchEvent(key('keydown', 'Space'));
  assert.deepEqual(input.sample(), { steer: -1, throttle: 1, brake: 0, handbrake: true });
  assert.deepEqual(actions, ['ArrowUp', 'KeyA', 'Space']);
  win.dispatchEvent(key('keyup', 'KeyA'));
  assert.equal(input.sample().steer, 0);
  win.dispatchEvent(new Event('blur'));
  assert.deepEqual(input.sample(), { steer: 0, throttle: 0, brake: 0, handbrake: false });
  assert.equal(input.device, 'teclado');
});

test('typing in a form field is not driving input', () => {
  const win = fakeWindow();
  const actions = [];
  const input = createKeyboardGamepadInput(win, (code) => actions.push(code));
  win.dispatchEvent(key('keydown', 'KeyW', { target: { tagName: 'INPUT' } }));
  win.dispatchEvent(key('keydown', 'KeyR', { target: { tagName: 'DIV', isContentEditable: true } }));
  assert.equal(input.sample().throttle, 0);
  assert.deepEqual(actions, []);
});

test('gamepad applies the deadzone, takes over when active and fires action edges', () => {
  const pads = [null, pad(0.56, { 7: 0.5, 3: 1 })];
  const win = fakeWindow(pads);
  const actions = [];
  const input = createKeyboardGamepadInput(win, (code) => actions.push(code));
  const frame = input.sample();
  assert.ok(Math.abs(frame.steer - 0.5) < 1e-9);
  assert.equal(frame.throttle, 0.5);
  assert.equal(input.device, 'mando');
  input.sample();
  assert.deepEqual(actions, ['KeyR'], 'Y fires once per press, not every frame');
  pads[1] = pad(0.1, {});
  assert.equal(input.sample().steer, 0, 'inside the deadzone');
  pads[1] = { ...pad(0, {}), mapping: '' };
  win.dispatchEvent(key('keydown', 'KeyS'));
  assert.equal(input.sample().brake, 1, 'non-standard pads are ignored');
});

test('dispose removes every listener', () => {
  const win = fakeWindow();
  const actions = [];
  const input = createKeyboardGamepadInput(win, (code) => actions.push(code));
  input.dispose();
  win.dispatchEvent(key('keydown', 'KeyW'));
  assert.equal(input.sample().throttle, 0);
  assert.deepEqual(actions, []);
});
