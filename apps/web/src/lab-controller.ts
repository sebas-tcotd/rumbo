// Lab controls: keyboard shortcuts and the actions behind the panel buttons. Same key map as the PoC.

import type { ProfileFile } from '@rumbo/contracts';
import type { CameraMode, ThreeRenderer } from '@rumbo/render-three';
import type { LabSession } from './lab-session.ts';

const CAMERAS: CameraMode[] = ['persecucion', 'cercana', 'aerea'];

/** Lab actions (structurally the `DevActions`/`DevEvents` of @rumbo/devtools) plus the key handler. */
export interface LabController {
  handleKey(code: string): void;
  setAutopilot(on: boolean): void;
  setPaused(on: boolean): void;
  setGhostOn(on: boolean): void;
  cycleCamera(): void;
  setPanelOpen(open: boolean): void;
  applyProfile(profile: ProfileFile): void;
  restartSession(): void;
  exportReplay(): void;
  verifyCurrentSession(): void;
  verifyReplayFile(json: string): void;
  switchTrack(id: string): void;
  /** AR28: car to lap distance `s` (marks the session altered). */
  teleport(s: number): void;
  /** AR28: re-simulates a replay up to `tick` and stays paused there. */
  seekReplay(json: string, tick: number): void;
  /** Replays exported for download (devtools subscribes); returns the unsubscribe function. */
  onReplay(listener: (json: string, ticks: number) => void): () => void;
  /** Exports the worker rejected (no replay will arrive); returns the unsubscribe function. */
  onExportFailed(listener: () => void): () => void;
}

/** Dependencies of the lab controller. */
export interface LabControllerDeps {
  session: LabSession;
  renderer: ThreeRenderer;
  switchTrack: (id: string) => void;
}

type ReplayActions = Pick<
  LabController,
  'exportReplay' | 'verifyCurrentSession' | 'verifyReplayFile' | 'seekReplay' | 'onReplay' | 'onExportFailed'
> & { cancelVerify(): void };

/** Replay export/verify flow: "verify this session" exports first and verifies the export it gets back. */
function createReplayActions(session: LabSession): ReplayActions {
  const replayListeners = new Set<(json: string, ticks: number) => void>();
  const failedListeners = new Set<() => void>();
  let verifyAfterExport = false;
  session.onCommandError = (operation) => {
    if (operation !== 'exportReplay') return;
    verifyAfterExport = false;
    for (const l of failedListeners) l();
  };
  session.onReplay = (json, ticks) => {
    if (verifyAfterExport) {
      verifyAfterExport = false;
      session.send({ type: 'verifyReplay', json });
      return;
    }
    for (const l of replayListeners) l(json, ticks);
  };
  const subscribe = <T>(set: Set<T>, listener: T) => {
    set.add(listener);
    return () => set.delete(listener);
  };
  return {
    cancelVerify: () => {
      verifyAfterExport = false;
    },
    exportReplay: () => session.send({ type: 'exportReplay' }),
    verifyCurrentSession() {
      verifyAfterExport = true;
      session.send({ type: 'exportReplay' });
    },
    verifyReplayFile: (json) => session.send({ type: 'verifyReplay', json }),
    // The worker pauses the seek's session itself and the session adopts that pause when it arrives,
    // so a rejected seek leaves the lab running as it was.
    seekReplay(json, tick) {
      verifyAfterExport = false;
      session.sendDev({ type: 'devSeekReplay', json, tick });
    },
    onReplay: (listener) => subscribe(replayListeners, listener),
    onExportFailed: (listener) => subscribe(failedListeners, listener),
  };
}

/** The PoC's lab shortcuts over the controller's actions. */
function keyMap(c: LabController, session: LabSession): Record<string, () => void> {
  const view = () => session.getSnapshot();
  return {
    KeyR: () => session.send({ type: 'reset' }),
    KeyT: () => session.send({ type: 'restart' }),
    KeyP: () => c.setAutopilot(!view().autopilot),
    KeyC: () => c.cycleCamera(),
    KeyG: () => c.setGhostOn(!view().ghostOn),
    KeyH: () => c.setPanelOpen(!view().panelOpen),
    Escape: () => c.setPaused(!view().paused),
  };
}

/** Wires lab state to the session (worker commands) and to the renderer's lab controls. */
export function createLabController({ session, renderer, switchTrack }: LabControllerDeps): LabController {
  const { cancelVerify, ...replay } = createReplayActions(session);
  const c: LabController = {
    ...replay,
    ...viewActions(session, renderer),
    applyProfile(profile) {
      cancelVerify();
      session.newSession(profile);
      renderer.setSuspensionRest(profile.params.suspension_rest_m);
    },
    restartSession() {
      const applied = session.getSnapshot().appliedProfile;
      if (applied) c.applyProfile(applied);
    },
    switchTrack,
    teleport: (s) => session.sendDev({ type: 'devTeleport', s }),
    handleKey: (code) => {
      if (Object.hasOwn(keys, code)) keys[code]();
    },
  };
  const keys = keyMap(c, session);
  return c;
}

type ViewActions = Pick<LabController, 'setAutopilot' | 'setPaused' | 'setGhostOn' | 'cycleCamera' | 'setPanelOpen'>;

/** Lab view toggles: each updates the store and the worker or renderer behind it. */
function viewActions(session: LabSession, renderer: ThreeRenderer): ViewActions {
  return {
    setAutopilot(on) {
      session.patch({ autopilot: on });
      session.send({ type: 'autopilot', on });
    },
    setPaused(on) {
      session.patch({ paused: on });
      session.send({ type: 'pause', on });
    },
    setGhostOn(on) {
      session.patch({ ghostOn: on });
      renderer.setGhostVisible(on);
    },
    cycleCamera() {
      const camera = session.getSnapshot().camera;
      const next = CAMERAS[(CAMERAS.indexOf(camera) + 1) % CAMERAS.length];
      session.patch({ camera: next });
      renderer.setCameraMode(next);
    },
    setPanelOpen: (open) => session.patch({ panelOpen: open }),
  };
}
