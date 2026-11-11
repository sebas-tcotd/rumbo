// Lab glue between the worker simulation and the views. One external store feeds both React roots
// (@rumbo/ui and @rumbo/devtools); it satisfies their snapshot types structurally. The render loop reads
// `latest` every frame; React only sees a ~15 Hz copy.

import {
  buildIndex,
  DEV_SEEK_MAX_CHARS,
  DEV_SEEK_MAX_TICKS,
  type DevCommand,
  type Hashes,
  type LapMsg,
  type ProfileFile,
  type ReadyMsg,
  type SimPort,
  type SimResponse,
  type StateIndex,
  type StateMsg,
  type ToWorker,
  type VerifyReport,
} from '@rumbo/contracts';
import type { CameraMode } from '@rumbo/render-three';
import type { UiNotice } from '@rumbo/ui';

/** A completed lap of the current session. */
export interface LapRecord {
  ticks: number;
  best: boolean;
}

/** Everything the lab views read. */
export interface LabView {
  ready: ReadyMsg | null;
  index: StateIndex | null;
  /** ~15 Hz copy of the latest state, safe to keep. */
  state: StateMsg | null;
  hashes: Hashes | null;
  appliedProfile: ProfileFile | null;
  laps: LapRecord[];
  ghost: Float32Array | null;
  verify: VerifyReport | null;
  verifyError: string | null;
  /** Technical message of a fatal error (simulation or renderer start-up). */
  error: string | null;
  /** Non-fatal rejection of a command (invalid profile, unsupported load, failed export). */
  notice: UiNotice | null;
  sessionNumber: number;
  /** A dev command (teleport, seek) changed this session: its replay no longer reproduces. */
  altered: boolean;
  // Lab controls, owned by the lab controller.
  autopilot: boolean;
  /** Manual pause (Esc); hiding the tab also pauses the worker but does not show the badge. */
  paused: boolean;
  camera: CameraMode;
  ghostOn: boolean;
  panelOpen: boolean;
  rendererReady: boolean;
  backend: string;
}

const INITIAL: LabView = {
  ready: null,
  index: null,
  state: null,
  hashes: null,
  appliedProfile: null,
  laps: [],
  ghost: null,
  verify: null,
  verifyError: null,
  error: null,
  notice: null,
  sessionNumber: 0,
  altered: false,
  autopilot: false,
  paused: false,
  camera: 'persecucion',
  ghostOn: true,
  panelOpen: true,
  rendererReady: false,
  backend: '—',
};

/** Builds the simulation port with the session's response handler (constructor injection). */
export type ConnectSim = (receive: (response: SimResponse) => void) => SimPort;

type Listener = () => void;
/** Minimum time between two React-visible state copies (~15 Hz). */
const UI_INTERVAL_MS = 66;

/** Why a seek would be refused before reaching the worker (same bounds as its wire validation). */
function seekProblem(json: string, tick: number): string | null {
  if (json.length > DEV_SEEK_MAX_CHARS) return 'El replay es demasiado grande para re-simularlo en el laboratorio.';
  if (!Number.isInteger(tick) || tick < 0 || tick > DEV_SEEK_MAX_TICKS) return `Tick fuera de rango: ${tick}.`;
  return null;
}

/** The profile embedded in a replay manifest, or null if it does not look like one. */
function replayProfile(json: string): ProfileFile | null {
  try {
    const profile = (JSON.parse(json) as { profile?: unknown }).profile as Partial<ProfileFile> | undefined;
    const valid =
      typeof profile?.id === 'string' && typeof profile.name === 'string' && typeof profile.params === 'object';
    return valid && profile.params !== null ? (profile as ProfileFile) : null;
  } catch {
    return null;
  }
}

/** Store of the lab session; the composition root wires its callbacks to the renderer and controller. */
export class LabSession {
  private readonly worker: SimPort;
  private manuallyPaused = false;
  private hidden = false;
  private readonly listeners = new Set<Listener>();
  private view: LabView = INITIAL;
  private autopilot = false;
  private pendingProfile: ProfileFile | null = null;
  /** A seek was sent and not answered yet: the session it creates starts out altered and paused. */
  private seekPending = false;
  /** `altered` as the current session started (a seek's session is born altered). */
  private sessionAltered = false;
  private disposed = false;
  private lastUiUpdate = 0;
  /** Latest state message; its prev/curr arrays are the sim client's scratch, valid until the next one. */
  latest: StateMsg | null = null;
  onReady: (m: ReadyMsg, index: StateIndex) => void = () => {};
  onGhost: (frames: Float32Array | null) => void = () => {};
  onReplay: (json: string, ticks: number) => void = () => {};
  /** Called when the worker rejects a command, so the host can drop state tied to it. */
  onCommandError: (operation: string) => void = () => {};
  /** Called when a confirmed session runs a different profile (applied, or a seek's replay profile). */
  onProfile: (profile: ProfileFile) => void = () => {};
  /** Called once when the lab stops with a fatal error, so the host can stop its frame loop. */
  onFatal: () => void = () => {};

  constructor(connect: ConnectSim) {
    this.worker = connect((msg) => this.handle(msg));
  }

  dispose(): void {
    if (this.disposed) return;
    this.disposed = true;
    this.worker.dispose();
  }

  /** Sends a lab command, bridging the PoC's message names to the SimPort protocol. */
  send(msg: ToWorker): void {
    if (this.disposed) return;
    switch (msg.type) {
      case 'init':
        this.worker.send({ type: 'load', zone: null, race: null, track: msg.trackJson, profile: msg.profileJson });
        break;
      case 'pause':
        this.manuallyPaused = msg.on;
        this.syncPause();
        break;
      case 'input':
        if (!this.hidden) this.worker.send({ type: 'input', frame: msg });
        break;
      case 'autopilot':
        // Remembered until the lab is ready: the worker resets it on every load.
        this.autopilot = msg.on;
        if (this.view.ready) this.worker.send(msg);
        break;
      default:
        this.worker.send(msg);
    }
  }

  /**
   * Sends an AR28 dev command and marks the session it affects as altered. Arguments the worker would
   * reject as an invalid command (which is fatal there) are refused here: a non-finite teleport is
   * ignored and an out-of-bounds seek is reported like a rejected replay.
   */
  sendDev(cmd: DevCommand): void {
    if (this.disposed) return;
    if (cmd.type === 'devTeleport') {
      if (!Number.isFinite(cmd.s)) return;
      this.patch({ altered: true });
    } else {
      const problem = seekProblem(cmd.json, cmd.tick);
      if (problem) {
        this.patch({ verifyError: problem });
        return;
      }
      this.seekPending = true;
      // The seek's session runs the replay's own profile: it becomes the applied one when it arrives.
      this.pendingProfile = replayProfile(cmd.json);
    }
    this.worker.send(cmd);
  }

  /** Tab visibility (P2): a hidden tab pauses the worker without undoing a manual pause. */
  setHidden(hidden: boolean): void {
    this.hidden = hidden;
    this.syncPause();
  }

  private syncPause(): void {
    if (this.view.ready) this.worker.send({ type: this.manuallyPaused || this.hidden ? 'pause' : 'resume' });
  }

  /** Loads the track with the initial profile. */
  init(trackJson: string, profile: ProfileFile): void {
    this.patch({ appliedProfile: profile });
    this.send({ type: 'init', trackJson, profileJson: JSON.stringify(profile) });
  }

  /** Applies a profile: a new session (car on the grid, replay from zero). */
  newSession(profile: ProfileFile): void {
    // UI state is reset when the worker confirms the new session, so a rejected profile changes nothing.
    this.pendingProfile = profile;
    this.patch({ notice: null });
    this.send({ type: 'newSession', profileJson: JSON.stringify(profile) });
  }

  /** Stops the lab with a fatal message (e.g. no WebGPU/WebGL). */
  fail(error: string): void {
    const first = !this.disposed;
    this.dispose();
    this.patch({ error: error || 'SIM_FAILED' });
    if (first) this.onFatal();
  }

  private handle(msg: SimResponse): void {
    switch (msg.type) {
      case 'ready':
        this.handleReady(msg);
        break;
      case 'state':
        this.handleState(msg);
        break;
      case 'lap':
        this.handleLap(msg);
        break;
      case 'session':
        this.handleSession(msg.hashes);
        break;
      case 'replay':
        this.onReplay(msg.json, msg.ticks);
        break;
      case 'verify':
        this.patch({ verify: msg.report, verifyError: null });
        break;
      case 'verifyError':
        // A rejected seek answers with verifyError and leaves the live session as it was.
        if (this.seekPending) this.pendingProfile = null;
        this.seekPending = false;
        this.patch({ verify: null, verifyError: msg.message });
        break;
      case 'commandError':
        this.handleCommandError(msg.code, msg.operation);
        break;
      case 'unsupported':
        this.dropDevCommand(msg.operation);
        this.patch({ notice: { code: 'UNSUPPORTED_OPERATION', operation: msg.operation } });
        this.onCommandError(msg.operation);
        break;
      case 'fatal':
        this.fail(msg.message);
        break;
    }
  }

  private handleReady(msg: ReadyMsg): void {
    try {
      const index = buildIndex(msg.layout);
      this.patch({ ready: msg, index, hashes: msg.hashes, error: null, notice: null });
      this.onReady(msg, index);
      this.syncPause();
      if (this.autopilot) this.worker.send({ type: 'autopilot', on: true });
    } catch (e) {
      this.fail(e instanceof Error ? e.message : String(e));
    }
  }

  private handleState(msg: StateMsg): void {
    this.latest = msg;
    const now = performance.now();
    if (now - this.lastUiUpdate <= UI_INTERVAL_MS) return;
    this.lastUiUpdate = now;
    this.patch({ state: { ...msg, prev: msg.prev.slice(), curr: msg.curr.slice() } });
  }

  private handleSession(hashes: Hashes): void {
    const seek = this.seekPending;
    // The worker publishes a single state for a paused session: it must not fall in the UI throttle.
    this.lastUiUpdate = Number.NEGATIVE_INFINITY;
    // A seek's session arrives paused in the worker; the host adopts it as a manual pause.
    if (seek) this.manuallyPaused = true;
    this.sessionAltered = seek;
    this.patch({
      hashes,
      sessionNumber: this.view.sessionNumber + 1,
      appliedProfile: this.pendingProfile ?? this.view.appliedProfile,
      laps: [],
      ghost: null,
      verify: null,
      verifyError: null,
      state: null,
      notice: null,
      altered: seek,
      ...(seek ? { paused: true } : {}),
    });
    const applied = this.pendingProfile;
    this.pendingProfile = null;
    this.seekPending = false;
    if (applied) this.onProfile(applied);
    if (this.view.ready) this.onGhost(null);
  }

  private handleCommandError(code: 'SIM_NOT_READY' | 'INVALID_PROFILE' | 'EXPORT_FAILED', operation: string): void {
    this.dropDevCommand(operation);
    // SIM_NOT_READY on startup input is harmless: it is sampled again once the lab is ready.
    if (code !== 'SIM_NOT_READY') {
      if (operation === 'newSession') this.pendingProfile = null;
      this.patch({ notice: { code } });
    }
    this.onCommandError(operation);
  }

  /** A rejected dev command leaves the session as it was: undo the marks set when it was sent. */
  private dropDevCommand(operation: string): void {
    if (operation === 'devSeekReplay') {
      this.seekPending = false;
      this.pendingProfile = null;
    }
    if (operation === 'devTeleport') this.patch({ altered: this.sessionAltered });
  }

  private handleLap(msg: LapMsg): void {
    const laps = [...this.view.laps, { ticks: msg.ticks, best: msg.best }];
    if (msg.ghost) this.onGhost(msg.ghost);
    this.patch({ laps, ghost: msg.ghost ?? this.view.ghost });
  }

  /** Merges a partial view and notifies both React roots. */
  patch(p: Partial<LabView>): void {
    this.view = { ...this.view, ...p };
    for (const l of this.listeners) l();
  }

  subscribe = (l: Listener): (() => void) => {
    this.listeners.add(l);
    return () => this.listeners.delete(l);
  };

  getSnapshot = (): LabView => this.view;
}
