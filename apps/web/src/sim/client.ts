// Cliente del worker en el hilo principal y un store mínimo para React.
// El render lee el estado en cada cuadro; React se actualiza a ~15 Hz.

import { createWorkerSim } from '@rumbo/sim-worker';
import type { SimPort, SimResponse } from '@rumbo/contracts';
import type { Hashes, LapMsg, ProfileFile, ReadyMsg, StateMsg, ToWorker, VerifyReport } from './protocol';
import { buildIndex, type StateIndex } from './state';

export interface LapRecord {
  ticks: number;
  best: boolean;
}

export interface LabSnapshot {
  ready: ReadyMsg | null;
  index: StateIndex | null;
  state: StateMsg | null;
  hashes: Hashes | null;
  appliedProfile: ProfileFile | null;
  laps: LapRecord[];
  hasGhost: boolean;
  verify: VerifyReport | null;
  verifyError: string | null;
  error: string | null;
  /** Non-fatal rejection of a command (invalid profile, unsupported load, failed export). */
  notice: string | null;
  sessionNumber: number;
}

type Listener = () => void;

const NOTICES = {
  SIM_NOT_READY: 'La simulación aún no está lista.',
  INVALID_PROFILE: 'El perfil no es válido; se mantiene la sesión actual.',
  EXPORT_FAILED: 'No se pudo exportar el replay.',
} as const;

export class SimClient {
  private worker: SimPort;
  private manuallyPaused = false;
  private hidden = false;
  private listeners = new Set<Listener>();
  private snap: LabSnapshot = {
    ready: null,
    index: null,
    state: null,
    hashes: null,
    appliedProfile: null,
    laps: [],
    hasGhost: false,
    verify: null,
    verifyError: null,
    error: null,
    notice: null,
    sessionNumber: 0,
  };
  private autopilot = false;
  private pendingProfile: ProfileFile | null = null;
  private disposed = false;
  private lastUiUpdate = 0;
  onState: (m: StateMsg) => void = () => {};
  onReady: (m: ReadyMsg, index: StateIndex) => void = () => {};
  onGhost: (frames: Float32Array | null) => void = () => {};
  onReplay: (json: string, ticks: number) => void = () => {};
  /** Called when the worker rejects a command, so the host can drop state tied to it. */
  onCommandError: (operation: string) => void = () => {};

  constructor() {
    this.worker = createWorkerSim(msg => this.handle(msg));
  }

  dispose() {
    if (this.disposed) return;
    this.disposed = true;
    this.worker.dispose();
  }

  send(msg: ToWorker) {
    if (this.disposed) return;
    switch (msg.type) {
      case 'init':
        this.worker.send({type:'load', zone:null, race:null, track:msg.trackJson, profile:msg.profileJson});
        break;
      case 'pause':
        this.manuallyPaused = msg.on;
        this.syncPause();
        break;
      case 'input':
        if (!this.hidden) this.worker.send({type:'input', frame: msg});
        break;
      case 'autopilot':
        // Remembered until the lab is ready: the worker resets it on every load.
        this.autopilot = msg.on;
        if (this.snap.ready) this.worker.send(msg);
        break;
      default:
        this.worker.send(msg);
    }
  }

  setHidden(hidden: boolean) {
    this.hidden = hidden;
    this.syncPause();
  }

  private syncPause() {
    if (this.snap.ready) this.worker.send({type: this.manuallyPaused || this.hidden ? 'pause' : 'resume'});
  }

  init(trackJson: string, profile: ProfileFile) {
    this.patch({ appliedProfile: profile });
    this.send({ type: 'init', trackJson, profileJson: JSON.stringify(profile) });
  }

  /** Aplica un perfil: es una sesión nueva (auto en la grilla, replay desde cero). */
  newSession(profile: ProfileFile) {
    // UI state is reset when the worker confirms the new session, so a rejected profile changes nothing.
    this.pendingProfile = profile;
    this.patch({ notice: null });
    this.send({ type: 'newSession', profileJson: JSON.stringify(profile) });
  }

  private handle(msg: SimResponse) {
    switch (msg.type) {
      case 'ready': {
        try {
          const index = buildIndex(msg.layout);
          this.patch({ ready: msg, index, hashes: msg.hashes, error: null, notice: null });
          this.onReady(msg, index);
          this.syncPause();
          if (this.autopilot) this.worker.send({ type: 'autopilot', on: true });
        } catch (e) {
          this.fail(e instanceof Error ? e.message : String(e));
        }
        break;
      }
      case 'state': {
        this.onState(msg);
        const now = performance.now();
        if (now - this.lastUiUpdate > 66) {
          this.lastUiUpdate = now;
          this.patch({ state: { ...msg, prev: msg.prev.slice(), curr: msg.curr.slice() } });
        }
        break;
      }
      case 'lap':
        this.handleLap(msg);
        break;
      case 'session':
        this.patch({
          hashes: msg.hashes,
          sessionNumber: this.snap.sessionNumber + 1,
          appliedProfile: this.pendingProfile ?? this.snap.appliedProfile,
          laps: [],
          hasGhost: false,
          verify: null,
          verifyError: null,
          state: null,
          notice: null,
        });
        this.pendingProfile = null;
        if (this.snap.ready) this.onGhost(null);
        break;
      case 'replay':
        this.onReplay(msg.json, msg.ticks);
        break;
      case 'verify':
        this.patch({ verify: msg.report, verifyError: null });
        break;
      case 'verifyError':
        this.patch({ verify: null, verifyError: msg.message });
        break;
      case 'commandError':
        // SIM_NOT_READY on startup input is harmless: it is sampled again once the lab is ready.
        if (msg.code !== 'SIM_NOT_READY') {
          if (msg.operation === 'newSession') this.pendingProfile = null;
          this.patch({ notice: NOTICES[msg.code] });
        }
        this.onCommandError(msg.operation);
        break;
      case 'unsupported':
        this.patch({ notice: 'Operación no disponible todavía: ' + msg.operation });
        this.onCommandError(msg.operation);
        break;
      case 'fatal':
        this.fail(msg.message);
        break;
    }
  }

  private fail(error: string) {
    this.dispose();
    this.patch({ error: error || 'SIM_FAILED' });
  }

  private handleLap(msg: LapMsg) {
    const laps = [...this.snap.laps, { ticks: msg.ticks, best: msg.best }];
    if (msg.ghost) this.onGhost(msg.ghost);
    this.patch({ laps, hasGhost: this.snap.hasGhost || !!msg.ghost });
  }

  private patch(p: Partial<LabSnapshot>) {
    this.snap = { ...this.snap, ...p };
    this.listeners.forEach((l) => l());
  }

  subscribe = (l: Listener) => {
    this.listeners.add(l);
    return () => this.listeners.delete(l);
  };

  getSnapshot = () => this.snap;
}
