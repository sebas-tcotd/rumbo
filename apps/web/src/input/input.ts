// Muestreo de teclado y mando. La forma de la dirección (rampa, retorno) vive en la
// simulación para que sea determinista: aquí el teclado envía -1, 0 o 1 tal cual.

export interface DriveInput {
  steer: number;
  throttle: number;
  brake: number;
  handbrake: boolean;
}

export type Device = 'teclado' | 'mando';

const DEADZONE = 0.12;

export class InputSampler {
  private keys = new Set<string>();
  private lastPadButtons: boolean[] = [];
  private onKeyAction: (code: string) => void;
  device: Device = 'teclado';

  constructor(onKeyAction: (code: string) => void) {
    this.onKeyAction = onKeyAction;
    window.addEventListener('keydown', this.down);
    window.addEventListener('keyup', this.up);
    window.addEventListener('blur', this.clear);
  }

  dispose() {
    window.removeEventListener('keydown', this.down);
    window.removeEventListener('keyup', this.up);
    window.removeEventListener('blur', this.clear);
  }

  private isTyping(e: KeyboardEvent) {
    const t = e.target as HTMLElement | null;
    return !!t && (t.tagName === 'INPUT' || t.tagName === 'TEXTAREA' || t.isContentEditable);
  }

  private down = (e: KeyboardEvent) => {
    if (this.isTyping(e)) return;
    if (DRIVE_KEYS.has(e.code)) e.preventDefault();
    if (!e.repeat) this.onKeyAction(e.code);
    this.keys.add(e.code);
    this.device = 'teclado';
  };

  private up = (e: KeyboardEvent) => {
    this.keys.delete(e.code);
  };

  private clear = () => this.keys.clear();

  sample(): DriveInput {
    const k = (...codes: string[]) => codes.some((c) => this.keys.has(c));
    const kb: DriveInput = {
      steer: (k('KeyD', 'ArrowRight') ? 1 : 0) - (k('KeyA', 'ArrowLeft') ? 1 : 0),
      throttle: k('KeyW', 'ArrowUp') ? 1 : 0,
      brake: k('KeyS', 'ArrowDown') ? 1 : 0,
      handbrake: k('Space'),
    };

    const pad = navigator.getGamepads?.().find((p) => p && p.connected && p.mapping === 'standard');
    if (!pad) return kb;
    const ax = pad.axes[0] ?? 0;
    const steer = Math.abs(ax) < DEADZONE ? 0 : Math.sign(ax) * ((Math.abs(ax) - DEADZONE) / (1 - DEADZONE));
    const pv: DriveInput = {
      steer,
      throttle: pad.buttons[7]?.value ?? 0,
      brake: pad.buttons[6]?.value ?? 0,
      handbrake: !!(pad.buttons[0]?.pressed || pad.buttons[5]?.pressed),
    };
    // Botones de acción del mando con detección de flanco: Y = recolocar,
    // Start = volver a la salida, Select = cámara.
    const pressed = pad.buttons.map((b) => b.pressed);
    if (pressed[3] && !this.lastPadButtons[3]) this.onKeyAction('KeyR');
    if (pressed[9] && !this.lastPadButtons[9]) this.onKeyAction('KeyT');
    if (pressed[8] && !this.lastPadButtons[8]) this.onKeyAction('KeyC');
    this.lastPadButtons = pressed;

    const padActive = pv.throttle > 0.02 || pv.brake > 0.02 || pv.steer !== 0 || pv.handbrake;
    if (padActive) this.device = 'mando';
    if (this.device === 'mando') return pv;
    return kb;
  }
}

const DRIVE_KEYS = new Set(['ArrowUp', 'ArrowDown', 'ArrowLeft', 'ArrowRight', 'Space']);
