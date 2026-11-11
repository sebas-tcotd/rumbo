// Lab controls: keyboard shortcuts and the actions behind the panel buttons. Same key map as the PoC.

import type { ProfileFile } from '@rumbo/contracts';
import type { CameraMode, ThreeRenderer } from '@rumbo/render-three';
import type { LabSession } from './lab-session.ts';

const CAMERAS: CameraMode[] = ['persecucion', 'cercana', 'aerea'];

/** Lab actions (structurally the `DevActions` of @rumbo/devtools) plus the key handler. */
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
  /** Replays exported for download (devtools subscribes); returns the unsubscribe function. */
  onReplay(listener: (json: string, ticks: number) => void): () => void;
}

/** Dependencies of the lab controller. */
export interface LabControllerDeps {
  session: LabSession;
  renderer: ThreeRenderer;
  switchTrack: (id: string) => void;
}

/** Wires lab state to the session (worker commands) and to the renderer's lab controls. */
export function createLabController({ session, renderer, switchTrack }: LabControllerDeps): LabController {
  const view = () => session.getSnapshot();
  const replayListeners = new Set<(json: string, ticks: number) => void>();
  let verifyAfterExport = false;

  session.onCommandError = (operation) => {
    if (operation === 'exportReplay') verifyAfterExport = false;
  };
  session.onReplay = (json, ticks) => {
    if (!verifyAfterExport) {
      for (const l of replayListeners) l(json, ticks);
      return;
    }
    verifyAfterExport = false;
    session.send({ type: 'verifyReplay', json });
  };

  const c: LabController = {
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
      const next = CAMERAS[(CAMERAS.indexOf(view().camera) + 1) % CAMERAS.length];
      session.patch({ camera: next });
      renderer.setCameraMode(next);
    },
    setPanelOpen: (open) => session.patch({ panelOpen: open }),
    applyProfile(profile) {
      verifyAfterExport = false;
      session.newSession(profile);
      renderer.setSuspensionRest(profile.params.suspension_rest_m);
    },
    restartSession() {
      const applied = view().appliedProfile;
      if (applied) c.applyProfile(applied);
    },
    exportReplay: () => session.send({ type: 'exportReplay' }),
    verifyCurrentSession() {
      verifyAfterExport = true;
      session.send({ type: 'exportReplay' });
    },
    verifyReplayFile: (json) => session.send({ type: 'verifyReplay', json }),
    switchTrack,
    onReplay(listener) {
      replayListeners.add(listener);
      return () => replayListeners.delete(listener);
    },
    handleKey: (code) => {
      if (Object.hasOwn(KEYS, code)) KEYS[code]();
    },
  };
  const KEYS: Record<string, () => void> = {
    KeyR: () => session.send({ type: 'reset' }),
    KeyT: () => session.send({ type: 'restart' }),
    KeyP: () => c.setAutopilot(!view().autopilot),
    KeyC: () => c.cycleCamera(),
    KeyG: () => c.setGhostOn(!view().ghostOn),
    KeyH: () => c.setPanelOpen(!view().panelOpen),
    Escape: () => c.setPaused(!view().paused),
  };
  return c;
}
