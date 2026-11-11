// The single display loop: samples input, then draws the interpolated state. Nothing allocates per
// frame on this path except the input message when the input actually changed.

import { type InputSourcePort, interpolationAlpha, type RendererPort, STATE_FIELDS } from '@rumbo/contracts';
import type { LabSession } from './lab-session.ts';

/** What the loop needs; all injected by the composition root. */
export interface FrameLoopDeps {
  session: LabSession;
  renderer: RendererPort;
  input: InputSourcePort;
  isHidden: () => boolean;
}

/** Handle of the running loop. */
export interface FrameLoop {
  /** Forgets the last sent input so the next sample is sent again (new session, tab shown again). */
  resetInput(): void;
  stop(): void;
}

/** Rendered before the first state arrives (the renderer ignores it until `build`). */
const EMPTY = new Float32Array(STATE_FIELDS.length);
/** Inputs are compared at the precision the PoC sent them with (3 decimals). */
const q = (v: number) => Math.round(v * 1000);

/** Starts the requestAnimationFrame loop. Call only after the renderer's `init()` resolved. */
export function startFrameLoop({ session, renderer, input, isHidden }: FrameLoopDeps): FrameLoop {
  let raf = 0;
  let hasSent = false;
  let steer = 0;
  let throttle = 0;
  let brake = 0;
  let handbrake = false;

  const sendInput = () => {
    // A hidden tab sends nothing and forgets the last input, so the car does not keep it on return.
    if (isHidden()) {
      hasSent = false;
      return;
    }
    const i = input.sample();
    const same = hasSent && q(i.steer) === steer && q(i.throttle) === throttle && q(i.brake) === brake;
    if (same && i.handbrake === handbrake) return;
    hasSent = true;
    steer = q(i.steer);
    throttle = q(i.throttle);
    brake = q(i.brake);
    handbrake = i.handbrake;
    session.send({ type: 'input', steer: i.steer, throttle: i.throttle, brake: i.brake, handbrake: i.handbrake });
  };

  const frame = () => {
    raf = requestAnimationFrame(frame);
    sendInput();
    const msg = session.latest;
    const ready = session.getSnapshot().ready;
    if (!msg || !ready) {
      renderer.frame(EMPTY, EMPTY, 0);
      return;
    }
    const alpha = interpolationAlpha(performance.timeOrigin + performance.now(), msg.at, ready.tickHz);
    renderer.frame(msg.prev, msg.curr, alpha);
  };
  raf = requestAnimationFrame(frame);

  return {
    resetInput: () => {
      hasSent = false;
    },
    stop: () => cancelAnimationFrame(raf),
  };
}
