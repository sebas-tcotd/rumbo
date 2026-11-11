// Keyboard and gamepad sampling. Steering shape (ramp, return) lives in the simulation so it stays
// deterministic: the keyboard sends -1, 0 or 1 as is.

import type { InputFrame, InputSourcePort } from '@rumbo/contracts';

/** Last device that produced driving input. */
export type InputDevice = 'teclado' | 'mando';

/** Keyboard + standard gamepad source; `device` tells which one is driving right now. */
export interface KeyboardGamepadInput extends InputSourcePort {
  readonly device: InputDevice;
}

const DEADZONE = 0.12;
const DRIVE_KEYS = new Set(['ArrowUp', 'ArrowDown', 'ArrowLeft', 'ArrowRight', 'Space']);
/** Gamepad action buttons (edge-triggered) mapped to the lab key they mimic: Y, Start, Select. */
const PAD_ACTIONS: [number, string][] = [
  [3, 'KeyR'],
  [9, 'KeyT'],
  [8, 'KeyC'],
];

function isTyping(e: KeyboardEvent): boolean {
  const t = e.target as HTMLElement | null;
  return !!t && (t.tagName === 'INPUT' || t.tagName === 'TEXTAREA' || t.isContentEditable);
}

const held = (keys: Set<string>, a: string, b?: string) => keys.has(a) || (b !== undefined && keys.has(b));

/** Writes the keyboard frame into `out` (no allocation: this runs every display frame). */
function keyboardFrame(keys: Set<string>, out: InputFrame): InputFrame {
  out.steer = (held(keys, 'KeyD', 'ArrowRight') ? 1 : 0) - (held(keys, 'KeyA', 'ArrowLeft') ? 1 : 0);
  out.throttle = held(keys, 'KeyW', 'ArrowUp') ? 1 : 0;
  out.brake = held(keys, 'KeyS', 'ArrowDown') ? 1 : 0;
  out.handbrake = held(keys, 'Space');
  return out;
}

/** Writes the gamepad frame into `out`: left stick with deadzone, triggers, A or RB handbrake. */
function padFrame(pad: Gamepad, out: InputFrame): InputFrame {
  const ax = pad.axes[0] ?? 0;
  out.steer = Math.abs(ax) < DEADZONE ? 0 : Math.sign(ax) * ((Math.abs(ax) - DEADZONE) / (1 - DEADZONE));
  out.throttle = pad.buttons[7]?.value ?? 0;
  out.brake = pad.buttons[6]?.value ?? 0;
  out.handbrake = !!(pad.buttons[0]?.pressed || pad.buttons[5]?.pressed);
  return out;
}

/** First connected gamepad with the standard mapping (other mappings have unknown layouts). */
function standardPad(pads: readonly (Gamepad | null)[] | undefined): Gamepad | null {
  if (!pads) return null;
  for (const p of pads) if (p?.connected && p.mapping === 'standard') return p;
  return null;
}

/**
 * Adapter of InputSourcePort over `window` key events and the first connected standard-mapping
 * gamepad. `onKeyAction(code)` fires on each non-repeated key press (and on gamepad action-button
 * edges); the lab key map lives in the composition root.
 */
export function createKeyboardGamepadInput(target: Window, onKeyAction: (code: string) => void): KeyboardGamepadInput {
  const keys = new Set<string>();
  const lastPad = PAD_ACTIONS.map(() => false);
  const kb: InputFrame = { steer: 0, throttle: 0, brake: 0, handbrake: false };
  const pv: InputFrame = { steer: 0, throttle: 0, brake: 0, handbrake: false };
  let device: InputDevice = 'teclado';
  const down = (e: KeyboardEvent) => {
    if (isTyping(e)) return;
    if (DRIVE_KEYS.has(e.code)) e.preventDefault();
    // Browser and OS shortcuts (Ctrl+R reload, Ctrl+P print…) are not lab actions.
    if (!e.repeat && !e.ctrlKey && !e.metaKey && !e.altKey) onKeyAction(e.code);
    keys.add(e.code);
    device = 'teclado';
  };
  const up = (e: KeyboardEvent) => keys.delete(e.code);
  const clear = () => keys.clear();
  target.addEventListener('keydown', down);
  target.addEventListener('keyup', up);
  target.addEventListener('blur', clear);

  const padActions = (pad: Gamepad) => {
    for (let i = 0; i < PAD_ACTIONS.length; i++) {
      const [button, code] = PAD_ACTIONS[i];
      const pressed = !!pad.buttons[button]?.pressed;
      if (pressed && !lastPad[i]) onKeyAction(code);
      lastPad[i] = pressed;
    }
  };
  /** Returns a frame owned by the adapter, overwritten by the next call: copy it to retain it. */
  const sample = (): InputFrame => {
    keyboardFrame(keys, kb);
    const pad = standardPad(target.navigator.getGamepads?.());
    if (!pad) return kb;
    padFrame(pad, pv);
    padActions(pad);
    if (pv.throttle > 0.02 || pv.brake > 0.02 || pv.steer !== 0 || pv.handbrake) device = 'mando';
    return device === 'mando' ? pv : kb;
  };
  return {
    sample,
    get device() {
      return device;
    },
    dispose() {
      target.removeEventListener('keydown', down);
      target.removeEventListener('keyup', up);
      target.removeEventListener('blur', clear);
    },
  };
}
