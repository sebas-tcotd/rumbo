import { useState, useSyncExternalStore } from 'react';
import { createPortal } from 'react-dom';
import type { DevActions, DevCameraMode, DevStore, DevTrack } from './dev-snapshot.ts';
import { SessionPanel } from './panels/SessionPanel.tsx';
import { TelemetryPanel } from './panels/TelemetryPanel.tsx';
import { TuningPanel } from './panels/TuningPanel.tsx';

type Tab = 'ajustes' | 'telemetria' | 'sesion';
const TABS: [Tab, string][] = [
  ['ajustes', 'Ajustes'],
  ['telemetria', 'Telemetría'],
  ['sesion', 'Sesión'],
];
const CAMERA_NAMES: Record<DevCameraMode, string> = {
  persecucion: 'persecución',
  cercana: 'cercana',
  aerea: 'aérea',
};

/** Props of {@link DevPanel}. */
export interface DevPanelProps {
  store: DevStore;
  actions: DevActions;
  track: DevTrack;
  tracks: DevTrack[];
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

/** Lab side panel: Tuning, Telemetry and Session tabs. Texts are literal Spanish (developer tool). */
export function DevPanel({ store, actions, track, tracks, toolbar }: DevPanelProps) {
  const snap = useSyncExternalStore(store.subscribe, store.getSnapshot);
  const [tab, setTab] = useState<Tab>('ajustes');
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
                onVerifyCurrent={actions.verifyCurrentSession}
                onVerifyFile={actions.verifyReplayFile}
                onRestart={actions.restartSession}
              />
            )}
          </div>
        </aside>
      )}
    </>
  );
}
