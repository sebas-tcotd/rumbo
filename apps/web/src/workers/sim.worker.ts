/// <reference lib="webworker" />
// Worker de simulación: dueño del reloj de paso fijo.
// Recibe la entrada más reciente y la asigna al próximo tick; nunca depende de
// que llegue un mensaje por tick.

import init, {
  Lab,
  default_profile,
  physics_version,
  profile_schema,
  state_layout,
  tick_hz,
  vehicle_geometry,
  verify_replay,
} from '@sim-wasm';
import {
  FLAG_HANDBRAKE,
  FLAG_RESET,
  FLAG_RESTART,
  GHOST_STRIDE,
  type FromWorker,
  type MeshData,
  type ToWorker,
} from '../sim/protocol';

const ctx = self as unknown as DedicatedWorkerGlobalScope;
const post = (msg: FromWorker, transfer: Transferable[] = []) => ctx.postMessage(msg, transfer);

let lab: Lab | null = null;
let trackJson = '';
let hz = 120;
let dtMs = 1000 / 120;
let I: Record<string, number> = {};

let input = { steer: 0, throttle: 0, brake: 0, handbrake: false };
let resetPending = false;
let restartPending = false;
let autopilot = false;
let paused = false;

let acc = 0;
let last = performance.now();
let stepMs = 0;
const tickTimes: number[] = [];

let lastState: Float32Array | null = null;
let beforeLast: Float32Array | null = null;

// Grabación de la vuelta en curso para el fantasma.
let rec = new Float32Array(GHOST_STRIDE * 120 * 120);
let recFrames = 0;
let prevLap = 0;
let prevLapTicks = 0;
let prevBest = -1;

function mesh(l: Lab, part: string): MeshData {
  return { positions: l.mesh_positions(part), surfaces: l.mesh_surfaces(part), indices: l.mesh_indices(part) };
}

function startSession(profileJson: string) {
  lab?.free();
  lab = new Lab(trackJson, profileJson, 1);
  lastState = beforeLast = null;
  recFrames = 0;
  prevLap = 0;
  prevLapTicks = 0;
  prevBest = -1;
  acc = 0;
  post({ type: 'session', hashes: { track: lab.track_hash(), profile: lab.profile_hash() } });
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

function loop() {
  const now = performance.now();
  const elapsed = now - last;
  last = now;
  if (lab && !paused) {
    acc += elapsed;
    if (acc > 250) acc = dtMs; // pestaña en segundo plano: no intentar ponerse al día
    let steps = 0;
    while (acc >= dtMs && steps < 12) {
      const t0 = performance.now();
      stepOnce(lab);
      const s = lab.state();
      stepMs = stepMs * 0.95 + (performance.now() - t0) * 0.05;
      recordGhost(s);
      beforeLast = lastState;
      lastState = s;
      tickTimes.push(now);
      acc -= dtMs;
      steps++;
    }
    while (tickTimes.length && now - tickTimes[0] > 1000) tickTimes.shift();
    if (steps > 0 && lastState) {
      post({
        type: 'state',
        prev: (beforeLast ?? lastState).slice(),
        curr: lastState.slice(),
        at: performance.timeOrigin + now - acc,
        stepMs,
        simHz: tickTimes.length,
        autopilot,
        paused,
      });
    }
  } else if (lab && lastState) {
    acc = 0;
  }
  setTimeout(loop, 2);
}

ctx.onmessage = async (ev: MessageEvent<ToWorker>) => {
  const msg = ev.data;
  try {
    switch (msg.type) {
      case 'init': {
        await init();
        trackJson = msg.trackJson;
        hz = tick_hz();
        dtMs = 1000 / hz;
        const layout: string[] = JSON.parse(state_layout());
        I = Object.fromEntries(layout.map((n, i) => [n, i]));
        startSession(msg.profileJson || default_profile());
        const l = lab!;
        post({
          type: 'ready',
          layout,
          schema: JSON.parse(profile_schema()),
          meshes: { drivable: mesh(l, 'drivable'), walls: mesh(l, 'walls'), decals: mesh(l, 'decals') },
          centerline: l.centerline(),
          groundY: l.ground_y(),
          trackLength: l.track_length(),
          checkpoints: Array.from(l.checkpoints()),
          track: JSON.parse(l.track_info()),
          vehicle: JSON.parse(vehicle_geometry()),
          hashes: { track: l.track_hash(), profile: l.profile_hash() },
          physicsVersion: physics_version(),
          tickHz: hz,
        });
        last = performance.now();
        loop();
        break;
      }
      case 'input':
        input = msg;
        break;
      case 'reset':
        resetPending = true;
        break;
      case 'restart':
        restartPending = true;
        break;
      case 'newSession':
        startSession(msg.profileJson);
        break;
      case 'autopilot':
        autopilot = msg.on;
        break;
      case 'pause':
        paused = msg.on;
        break;
      case 'exportReplay':
        if (lab) post({ type: 'replay', json: lab.replay_json(), ticks: lab.tick() });
        break;
      case 'verifyReplay': {
        const t0 = performance.now();
        const report = JSON.parse(verify_replay(trackJson, msg.json));
        report.message += ` (${Math.round(performance.now() - t0)} ms)`;
        post({ type: 'verify', report });
        break;
      }
    }
  } catch (e) {
    post({ type: 'error', message: e instanceof Error ? e.message : String(e) });
  }
};
