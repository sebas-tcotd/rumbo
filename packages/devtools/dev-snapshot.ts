import type { Hashes, ProfileFile, ReadyMsg, StateIndex, StateMsg, VerifyReport } from '@rumbo/contracts';

/** Lab camera presets (same names as the renderer's, declared here so devtools never imports it). */
export type DevCameraMode = 'persecucion' | 'cercana' | 'aerea';

/** A completed lap of the current session. */
export interface DevLap {
  ticks: number;
  best: boolean;
}

/**
 * What the lab panels read from the host, structurally: apps/web's lab store satisfies it without
 * devtools importing the app. `state` is the ~15 Hz capture, never the per-frame scratch.
 */
export interface DevSnapshot {
  ready: ReadyMsg | null;
  index: StateIndex | null;
  state: StateMsg | null;
  hashes: Hashes | null;
  appliedProfile: ProfileFile | null;
  laps: DevLap[];
  verify: VerifyReport | null;
  verifyError: string | null;
  autopilot: boolean;
  ghostOn: boolean;
  camera: DevCameraMode;
  panelOpen: boolean;
  /** Rendering backend shown in telemetry ('WebGPU', 'WebGL 2'…). */
  backend: string;
}

/** Lab commands the panels can trigger; implemented by apps/web's lab controller. */
export interface DevActions {
  setAutopilot(on: boolean): void;
  setGhostOn(on: boolean): void;
  cycleCamera(): void;
  setPanelOpen(open: boolean): void;
  /** Starts a new session with this profile (the car goes back to the grid). */
  applyProfile(profile: ProfileFile): void;
  restartSession(): void;
  exportReplay(): void;
  verifyCurrentSession(): void;
  verifyReplayFile(json: string): void;
  switchTrack(id: string): void;
}

/** A compiler diagnostic attached to a track. */
export interface DevTrackDiagnostic {
  severity: 'info' | 'warning' | 'error';
  code: string;
  message: string;
  at?: { lat: number; lon: number };
}

/** Minimal track entry the session panel lists (the app keeps the full catalogue). */
export interface DevTrack {
  id: string;
  name: string;
  closed: boolean;
  /** Only tracks generated from real data. */
  status?: 'VALID' | 'DEGRADED' | 'UNSUPPORTED';
  stats?: Record<string, number>;
  diagnostics: DevTrackDiagnostic[];
  attribution?: string;
}

/** Minimal external store (the shape `useSyncExternalStore` consumes). */
export interface DevStore {
  subscribe(listener: () => void): () => void;
  getSnapshot(): DevSnapshot;
}

/** Replays exported by the worker reach devtools here (it downloads them). */
export interface DevEvents {
  onReplay(listener: (json: string, ticks: number) => void): () => void;
}
