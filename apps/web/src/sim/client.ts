// Cliente del worker en el hilo principal y un store mínimo para React.
// El render lee el estado en cada cuadro; React se actualiza a ~15 Hz.

import type { FromWorker, Hashes, LapMsg, ProfileFile, ReadyMsg, StateMsg, ToWorker, VerifyReport } from './protocol';
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
  error: string | null;
  sessionNumber: number;
}

type Listener = () => void;

export class SimClient {
  private worker: Worker;
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
    error: null,
    sessionNumber: 0,
  };
  private lastUiUpdate = 0;
  onState: (m: StateMsg) => void = () => {};
  onReady: (m: ReadyMsg, index: StateIndex) => void = () => {};
  onGhost: (frames: Float32Array | null) => void = () => {};
  onReplay: (json: string, ticks: number) => void = () => {};

  constructor() {
    this.worker = new Worker(new URL('../workers/sim.worker.ts', import.meta.url), { type: 'module' });
    this.worker.onmessage = (ev: MessageEvent<FromWorker>) => this.handle(ev.data);
    this.worker.onerror = (ev) => this.patch({ error: `El worker falló: ${ev.message || 'error desconocido'}` });
  }

  dispose() {
    this.worker.terminate();
  }

  send(msg: ToWorker) {
    this.worker.postMessage(msg);
  }

  init(trackJson: string, profile: ProfileFile) {
    this.patch({ appliedProfile: profile });
    this.send({ type: 'init', trackJson, profileJson: JSON.stringify(profile) });
  }

  /** Aplica un perfil: es una sesión nueva (auto en la grilla, replay desde cero). */
  newSession(profile: ProfileFile) {
    this.patch({ appliedProfile: profile, laps: [], hasGhost: false });
    this.onGhost(null);
    this.send({ type: 'newSession', profileJson: JSON.stringify(profile) });
  }

  private handle(msg: FromWorker) {
    switch (msg.type) {
      case 'ready': {
        try {
          const index = buildIndex(msg.layout);
          this.patch({ ready: msg, index, hashes: msg.hashes, error: null });
          this.onReady(msg, index);
        } catch (e) {
          this.patch({ error: e instanceof Error ? e.message : String(e) });
        }
        break;
      }
      case 'state': {
        this.onState(msg);
        const now = performance.now();
        if (now - this.lastUiUpdate > 66) {
          this.lastUiUpdate = now;
          this.patch({ state: msg });
        } else {
          this.snap.state = msg; // sin notificar
        }
        break;
      }
      case 'lap':
        this.handleLap(msg);
        break;
      case 'session':
        this.patch({ hashes: msg.hashes, sessionNumber: this.snap.sessionNumber + 1 });
        break;
      case 'replay':
        this.onReplay(msg.json, msg.ticks);
        break;
      case 'verify':
        this.patch({ verify: msg.report });
        break;
      case 'error':
        this.patch({ error: msg.message });
        break;
    }
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
