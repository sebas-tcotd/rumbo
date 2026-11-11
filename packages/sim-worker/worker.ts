/// <reference lib="webworker" />
// Worker de simulación: dueño del reloj de paso fijo.
// Recibe la entrada más reciente y la asigna al próximo tick; nunca depende de
// que llegue un mensaje por tick.

import init, {
  Lab,
  hash_content,
  default_profile,
  physics_version,
  profile_schema,
  state_layout,
  snapshot_descriptor,
  tick_hz,
  vehicle_geometry,
  verify_replay,
} from '@rumbo/sim-wasm';
import {
  FLAG_HANDBRAKE,
  FLAG_RESET,
  FLAG_RESTART,
  GHOST_STRIDE,
  type MeshData,
  type HashReply,
  type CommandMessage,
  type ResponseMessage,
  type SimResponse,
  type SnapshotMessage,
  PROTOCOL_VERSION,
  SNAPSHOT_VERSION,
  SNAPSHOT_BYTES,
  STATE_FIELDS,
  buildIndex,
  validateSnapshotDescriptor,
  isCommand,
} from '@rumbo/contracts';

import { verifyReplay } from './verify-replay';

import { FixedClock } from './clock';
import { SnapshotPool } from './pool';

const ctx = self as unknown as DedicatedWorkerGlobalScope;
let generation = 0;
const pool = new SnapshotPool(SNAPSHOT_BYTES);
const post = (
  msg: Exclude<SimResponse, { type: 'state' }> | HashReply | SnapshotMessage,
  transfer: Transferable[] = [],
) =>
  ctx.postMessage(
    { ...msg, protocolVersion: PROTOCOL_VERSION, generation } satisfies ResponseMessage,
    transfer,
  );
let timer: ReturnType<typeof setTimeout> | undefined;
let memory: WebAssembly.Memory;
let borrowed: Float32Array | undefined;
let initialization: ReturnType<typeof init> | undefined;

let lab: Lab | null = null;
let trackJson = '';
let hz = 120;
let clock = new FixedClock(120);
let I: Record<string, number> = {};

let input = { steer: 0, throttle: 0, brake: 0, handbrake: false };
let resetPending = false;
let restartPending = false;
let autopilot = false;
let paused = true;

let stepMs = 0;
const tickTimes: number[] = [];

let lastState = new Float32Array(STATE_FIELDS.length);
let beforeLast = new Float32Array(STATE_FIELDS.length);
let hasState = false;

// Grabación de la vuelta en curso para el fantasma.
let rec = new Float32Array(GHOST_STRIDE * 120 * 120);
let recFrames = 0;
let prevLap = 0;
let prevLapTicks = 0;
let prevBest = -1;

function mesh(l: Lab, part: string): MeshData {
  return {
    positions: l.mesh_positions(part),
    surfaces: l.mesh_surfaces(part),
    indices: l.mesh_indices(part),
  };
}

/** Latest executed state not yet published because no transport slot was free (or a pause/session needs one). */
let dirty = false;
/** Absolute time at which `lastState` was due. */
let lastAt = 0;

/** Copy the Lab state into the two interpolation scratch arrays. Views over WASM memory never escape. */
function capture(l: Lab) {
  const ptr = l.state_ptr();
  if (!borrowed || borrowed.buffer !== memory.buffer || borrowed.byteOffset !== ptr) {
    borrowed = new Float32Array(memory.buffer, ptr, STATE_FIELDS.length);
  }
  const scratch = beforeLast;
  beforeLast = lastState;
  lastState = scratch;
  lastState.set(borrowed);
  if (!hasState) beforeLast.set(lastState);
  hasState = true;
}

/** Publish the latest state if a transport slot is free; otherwise it stays dirty until one returns. */
function publish() {
  if (!lab || !hasState || !dirty) return;
  const lease = pool.take();
  if (!lease) return;
  const values = new Float32Array(lease.buffer);
  values.set(beforeLast);
  values.set(lastState, STATE_FIELDS.length);
  dirty = false;
  post(
    {
      type: 'snapshot',
      ...lease,
      layoutVersion: SNAPSHOT_VERSION,
      at: lastAt,
      stepMs,
      simHz: tickTimes.length,
      autopilot,
      paused,
    },
    [lease.buffer],
  );
}

/**
 * Replace the session. The new Lab is built first so a rejected profile (thrown string) leaves the live
 * session untouched; traps and other errors still propagate to `fatal`.
 */
function startSession(profileJson: string, showInitial: boolean) {
  replaceLab(new Lab(trackJson, profileJson, 1), showInitial);
}

/** Install an already-built Lab as the live session and announce it. */
function replaceLab(next: Lab, showInitial: boolean) {
  lab?.free();
  lab = next;
  hasState = false;
  dirty = false;
  resetPending = restartPending = false;
  tickTimes.length = 0;
  borrowed = undefined;
  recFrames = 0;
  prevLap = 0;
  prevLapTicks = 0;
  prevBest = -1;
  if (!paused) clock.start(performance.now());
  post({ type: 'session', hashes: { track: next.track_hash(), profile: next.profile_hash() } });
  if (paused && showInitial) {
    // No tick will run while paused: show the new session's initial state.
    capture(next);
    lastAt = performance.timeOrigin + performance.now();
    dirty = true;
    publish();
  }
}

function currentLab(): Lab {
  if (!lab) throw new Error('SIM_NOT_READY');
  return lab;
}

function quant(v: number, lo: number, hi: number) {
  return Math.max(lo, Math.min(hi, Math.round(v)));
}

function stepOnce(l: Lab) {
  let steer: number;
  let throttle: number;
  let brake: number;
  let flags: number;
  if (autopilot) {
    const p = l.autopilot_input();
    steer = ((p & 0xff) << 24) >> 24; // i8 con signo
    throttle = (p >> 8) & 0xff;
    brake = (p >> 16) & 0xff;
    flags = (p >>> 24) & 0xff;
  } else {
    steer = quant(input.steer * 127, -127, 127);
    throttle = quant(input.throttle * 255, 0, 255);
    brake = quant(input.brake * 255, 0, 255);
    flags = input.handbrake ? FLAG_HANDBRAKE : 0;
  }
  if (resetPending) {
    flags |= FLAG_RESET;
    resetPending = false;
  }
  if (restartPending) {
    flags |= FLAG_RESTART;
    restartPending = false;
  }
  l.step(steer, throttle, brake, flags);
}

function recordGhost(s: Float32Array) {
  const lap = s[I.lap];
  const lapTicks = s[I.lapTicks];
  const best = s[I.bestLapTicks];
  if (lapTicks < prevLapTicks) {
    // Empezó una vuelta nueva (completa, anulada o la primera).
    if (lap > prevLap) {
      const isBest = best !== prevBest;
      const ghost = isBest ? rec.slice(0, recFrames * GHOST_STRIDE) : null;
      post({ type: 'lap', ticks: s[I.lastLapTicks], best: isBest, ghost }, ghost ? [ghost.buffer] : []);
    }
    recFrames = 0;
  }
  prevLap = lap;
  prevLapTicks = lapTicks;
  prevBest = best;

  if (s[I.nextCheckpoint] < 1) return; // cronómetro aún no iniciado
  if ((recFrames + 1) * GHOST_STRIDE > rec.length) {
    const bigger = new Float32Array(rec.length * 2);
    bigger.set(rec);
    rec = bigger;
  }
  const o = recFrames * GHOST_STRIDE;
  rec[o] = s[I.px];
  rec[o + 1] = s[I.py];
  rec[o + 2] = s[I.pz];
  rec[o + 3] = s[I.qx];
  rec[o + 4] = s[I.qy];
  rec[o + 5] = s[I.qz];
  rec[o + 6] = s[I.qw];
  rec[o + 7] = s[I.progressS];
  recFrames++;
}

/** Show the latest state while paused (no tick will publish it). */
function publishPaused(l: Lab) {
  capture(l);
  lastAt = performance.timeOrigin + performance.now();
  dirty = true;
  publish();
}

/** AR28: move the car to lap distance `s`. Not recorded in the replay (the host marks the session). */
function devTeleport(l: Lab, s: number) {
  l.teleport(s);
  // Both interpolation states are the new pose, so the renderer snaps instead of sweeping across the map.
  hasState = false;
  if (paused) publishPaused(l);
}

/**
 * AR28: replace the session with a replay re-simulated up to `tick`, paused there. A rejected replay
 * (thrown string) is reported like a failed verification and leaves the live session untouched.
 */
function devSeekReplay(json: string, tick: number) {
  let next: Lab;
  try {
    next = Lab.from_replay(trackJson, json, tick);
  } catch (cause) {
    if (typeof cause !== 'string') throw cause;
    post({ type: 'verifyError', code: 'INVALID_REPLAY', message: cause });
    return;
  }
  paused = true;
  clearTimeout(timer);
  clock.stop();
  replaceLab(next, true);
}

let failed = false;
function fatal(cause: unknown) {
  if (failed) return;
  failed = true;
  try {
    post({
      type: 'fatal',
      code: 'SIM_FAILED',
      message: (cause instanceof Error ? cause.message : String(cause)) || 'SIM_FAILED',
    });
  } catch {
    // The channel itself failed; closing the worker makes the host's onerror/terminate path take over.
  } finally {
    ctx.close();
  }
}

function loop() {
  if (failed) return;
  try {
    wake();
  } catch (cause) {
    fatal(cause);
  }
}

function wake() {
  const now = performance.now();
  if (lab && !paused) {
    const steps = clock.advance(now);
    for (let i = 0; i < steps; i++) {
      const t0 = performance.now();
      stepOnce(lab);
      capture(lab);
      stepMs = stepMs * 0.95 + (performance.now() - t0) * 0.05;
      recordGhost(lastState);
      tickTimes.push(now);
    }
    while (tickTimes.length && now - tickTimes[0] > 1000) tickTimes.shift();
    if (steps) {
      dirty = true;
      lastAt = performance.timeOrigin + now - clock.remainder;
    }
    publish();
  }
  if (!paused) timer = setTimeout(loop, 2);
}

ctx.onmessageerror = () => fatal('INVALID_WORKER_MESSAGE');

ctx.onmessage = async (ev: MessageEvent<CommandMessage>) => {
  if (failed) return;
  const msg = ev.data;
  let loadingGeneration: number | undefined;
  try {
    if (!isCommand(msg)) throw new Error('INVALID_COMMAND');
    if (msg.type === 'returnBuffer') {
      pool.release(msg.slot, msg.buffer);
      publish();
      return;
    }
    if (msg.type === 'load' && (msg.zone !== null || msg.race !== null)) {
      // Reserved for later epics: reject before touching the live session.
      post({ type: 'unsupported', operation: 'load', code: 'UNSUPPORTED_CAPABILITY' });
      return;
    }
    if (msg.type === 'newSession' && !lab) {
      post({ type: 'commandError', operation: msg.type, code: 'SIM_NOT_READY' });
      return;
    }
    if (msg.type === 'hashContent') {
      // Stateless: not tied to a session, so it is answered whatever the current generation is.
      initialization ??= init();
      await initialization;
      if (!failed) post({ type: 'contentHash', requestId: msg.requestId, hash: hash_content(msg.bytes) });
      return;
    }
    if (msg.type === 'load' || msg.type === 'newSession') {
      if (msg.generation <= generation) return;
      generation = msg.generation;
    } else if (msg.generation !== generation) return;
    if (!lab && !['load', 'dispose', 'hashContent', 'loadGhost'].includes(msg.type)) {
      post({ type: 'commandError', operation: msg.type, code: 'SIM_NOT_READY' });
      return;
    }
    switch (msg.type) {
      case 'load': {
        loadingGeneration = generation;
        clearTimeout(timer);
        paused = true;
        clock.stop();
        lab?.free();
        lab = null;
        hasState = false;
        resetPending = restartPending = autopilot = false;
        input = { steer: 0, throttle: 0, brake: 0, handbrake: false };
        initialization ??= init();
        const wasm = await initialization;
        if (failed || generation !== msg.generation) return;
        memory = wasm.memory;
        trackJson = msg.track;
        hz = tick_hz();
        clock = new FixedClock(hz);
        const layout: string[] = JSON.parse(state_layout());
        validateSnapshotDescriptor(JSON.parse(snapshot_descriptor()));
        I = buildIndex(layout);
        startSession(msg.profile || default_profile(), false);
        const l: Lab = currentLab();
        post({
          type: 'ready',
          layout,
          schema: JSON.parse(profile_schema()),
          meshes: { drivable: mesh(l, 'drivable'), walls: mesh(l, 'walls'), decals: mesh(l, 'decals') },
          centerline: l.centerline(),
          barriers: JSON.parse(l.barriers_json()),
          groundY: l.ground_y(),
          trackLength: l.track_length(),
          checkpoints: Array.from(l.checkpoints()),
          track: JSON.parse(l.track_info()),
          vehicle: JSON.parse(vehicle_geometry()),
          hashes: { track: l.track_hash(), profile: l.profile_hash() },
          physicsVersion: physics_version(),
          tickHz: hz,
        });

        break;
      }
      case 'input':
        input = msg.frame;
        break;
      case 'reset':
        resetPending = true;
        break;
      case 'restart':
        restartPending = true;
        break;
      case 'newSession':
        try {
          startSession(msg.profileJson, true);
        } catch (e) {
          if (typeof e !== 'string') throw e;
          post({ type: 'commandError', operation: 'newSession', code: 'INVALID_PROFILE' });
        }
        break;
      case 'autopilot':
        autopilot = msg.on;
        break;
      case 'dispose':
        clearTimeout(timer);
        failed = true;
        lab?.free();
        ctx.close();
        break;
      case 'loadGhost':
        post({ type: 'unsupported', operation: msg.type, code: 'UNSUPPORTED_CAPABILITY' });
        break;
      case 'start':
      case 'resume':
        if (lab && paused) {
          paused = false;
          clock.start(performance.now());
          loop();
        }
        break;
      case 'pause':
        paused = true;
        clearTimeout(timer);
        clock.stop();
        if (hasState) {
          dirty = true;
          publish();
        }
        break;
      case 'exportReplay':
        if (lab) {
          try {
            post({ type: 'replay', json: lab.replay_json(), ticks: lab.tick() });
          } catch (e) {
            if (typeof e !== 'string') throw e;
            post({ type: 'commandError', operation: 'exportReplay', code: 'EXPORT_FAILED' });
          }
        }
        break;
      case 'devTeleport':
        devTeleport(currentLab(), msg.s);
        break;
      case 'devSeekReplay':
        devSeekReplay(msg.json, msg.tick);
        break;
      case 'verifyReplay': {
        const t0 = performance.now();
        const result = verifyReplay(verify_replay, trackJson, msg.json);
        if (result.type === 'verify')
          result.report.message += ' (' + Math.round(performance.now() - t0) + ' ms)';
        post(result);
        break;
      }
      default:
        throw new Error('INVALID_COMMAND');
    }
  } catch (e) {
    if (loadingGeneration !== undefined && loadingGeneration !== generation) return;
    fatal(e);
  }
};
