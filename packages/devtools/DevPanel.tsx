import { DEV_SEEK_MAX_CHARS, DEV_SEEK_MAX_TICKS, type OverlayKind, type RenderDebugPort } from '@rumbo/contracts';
import { useEffect, useRef, useState, useSyncExternalStore } from 'react';
import { createPortal } from 'react-dom';
import type { DevActions, DevCameraMode, DevEvents, DevSnapshot, DevStore, DevTrack } from './dev-snapshot.ts';
import { DebugPanel } from './panels/DebugPanel.tsx';
import { download } from './panels/download.ts';
import { SessionPanel } from './panels/SessionPanel.tsx';
import { TelemetryPanel } from './panels/TelemetryPanel.tsx';
import { TuningPanel } from './panels/TuningPanel.tsx';

type Tab = 'ajustes' | 'telemetria' | 'sesion' | 'depuracion';
const TABS: [Tab, string][] = [
  ['ajustes', 'Ajustes'],
  ['telemetria', 'Telemetría'],
  ['sesion', 'Sesión'],
  ['depuracion', 'Depuración'],
];
const NO_OVERLAYS: Record<OverlayKind, boolean> = { axis: false, checkpoints: false, collision: false };
const CAMERA_NAMES: Record<DevCameraMode, string> = {
  persecucion: 'persecución',
  cercana: 'cercana',
  aerea: 'aérea',
};

/** Props of {@link DevPanel}. */
export interface DevPanelProps {
  store: DevStore;
  actions: DevActions;
  events: DevEvents;
  track: DevTrack;
  tracks: DevTrack[];
  debug: RenderDebugPort;
  /** Element over the 3D view that hosts the show/hide button. */
  toolbar: HTMLElement;
}

/** Show/hide button rendered over the 3D view. */
function PanelToggle({ open, actions, toolbar }: { open: boolean; actions: DevActions; toolbar: HTMLElement }) {
  return createPortal(
    <button className="panel-toggle" onClick={() => actions.setPanelOpen(!open)} aria-expanded={open}>
      {open ? 'Ocultar panel' : 'Mostrar panel'}
    </button>,
    toolbar,
  );
}

/**
 * Replay files: downloads exported replays (as `<track>.<case>.json` when recording a golden), remembers
 * the last verified file and re-simulates it up to its first mismatch (AR28).
 */
function useReplayFiles(snap: DevSnapshot, actions: DevActions, events: DevEvents, track: DevTrack) {
  const golden = useRef<string | null>(null);
  const [lastFile, setLastFile] = useState<string | null>(null);
  const [seekError, setSeekError] = useState<string | null>(null);
  useEffect(
    () =>
      events.onReplay((json, ticks) => {
        const name = golden.current;
        golden.current = null;
        download(name ? `${track.id}.${name}.json` : `replay-${ticks}-ticks.json`, json);
      }),
    [events, track.id],
  );
  const mismatch = snap.verify && !snap.verify.ok ? snap.verify.firstMismatchTick : null;
  const seekTick = lastFile !== null && mismatch !== null ? mismatch : null;
  return {
    seekTick,
    seekError,
    recordGolden(name: string) {
      golden.current = name;
      actions.exportReplay();
    },
    verifyFile(json: string) {
      setLastFile(json);
      setSeekError(null);
      actions.verifyReplayFile(json);
    },
    verifyCurrent() {
      setLastFile(null);
      actions.verifyCurrentSession();
    },
    seek() {
      if (lastFile === null || seekTick === null) return;
      if (lastFile.length > DEV_SEEK_MAX_CHARS || seekTick > DEV_SEEK_MAX_TICKS) {
        setSeekError('El replay es demasiado grande para re-simularlo en el laboratorio.');
        return;
      }
      actions.seekReplay(lastFile, seekTick);
    },
  };
}

/** Lab side panel: Tuning, Telemetry, Session and Debug tabs. Texts are literal Spanish (developer tool). */
export function DevPanel({ store, actions, events, track, tracks, debug, toolbar }: DevPanelProps) {
  const snap = useSyncExternalStore(store.subscribe, store.getSnapshot);
  const [tab, setTab] = useState<Tab>('ajustes');
  const [overlays, setOverlays] = useState(NO_OVERLAYS);
  const files = useReplayFiles(snap, actions, events, track);
  const km = snap.ready ? `${(snap.ready.trackLength / 1000).toFixed(2)} km` : '…';
  return (
    <>
      <PanelToggle open={snap.panelOpen} actions={actions} toolbar={toolbar} />
      {snap.panelOpen && (
        <aside className="panel" aria-label="Laboratorio">
          <header className="panel-head">
            <h1>Laboratorio de manejo</h1>
            <p>{`${track.name} · ${km}`}</p>
          </header>
          <nav className="tabs" role="tablist">
            {TABS.map(([id, label]) => (
              <button key={id} role="tab" aria-selected={tab === id} onClick={() => setTab(id)}>
                {label}
              </button>
            ))}
          </nav>
          <div className="panel-body">
            {tab === 'ajustes' && snap.ready && snap.appliedProfile && (
              <TuningPanel schema={snap.ready.schema} applied={snap.appliedProfile} onApply={actions.applyProfile} />
            )}
            {tab === 'telemetria' && <TelemetryPanel snap={snap} />}
            {tab === 'depuracion' && (
              <DebugPanel
                snap={snap}
                actions={actions}
                debug={debug}
                overlays={overlays}
                onOverlays={setOverlays}
                onRecordGolden={files.recordGolden}
              />
            )}
            {tab === 'sesion' && (
              <SessionPanel
                snap={snap}
                track={track}
                tracks={tracks}
                onTrack={actions.switchTrack}
                autopilot={snap.autopilot}
                onAutopilot={actions.setAutopilot}
                ghostOn={snap.ghostOn}
                onGhost={actions.setGhostOn}
                camera={CAMERA_NAMES[snap.camera]}
                onCamera={actions.cycleCamera}
                onExport={actions.exportReplay}
                onVerifyCurrent={files.verifyCurrent}
                onVerifyFile={files.verifyFile}
                onRestart={actions.restartSession}
                seekTick={files.seekTick}
                onSeek={files.seek}
                seekError={files.seekError}
              />
            )}
          </div>
        </aside>
      )}
    </>
  );
}
