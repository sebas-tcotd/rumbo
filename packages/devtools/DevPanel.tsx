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
 * Downloads exported replays, as `<track>.<case>.json` when the export was asked for as a golden. The
 * golden name is dropped when the export fails, so it never renames a later ordinary download.
 */
function useReplayDownloads(actions: DevActions, events: DevEvents, track: DevTrack) {
  const golden = useRef<string | null>(null);
  useEffect(() => {
    const stopReplay = events.onReplay((json, ticks) => {
      const name = golden.current;
      golden.current = null;
      download(name ? `${track.id}.${name}.json` : `replay-${ticks}-ticks.json`, json);
    });
    const stopFailed = events.onExportFailed(() => {
      golden.current = null;
    });
    return () => {
      stopReplay();
      stopFailed();
    };
  }, [events, track.id]);
  return (name: string) => {
    golden.current = name;
    actions.exportReplay();
  };
}

/** A verified file and the report that was on screen when it was sent (stale until a new one arrives). */
interface VerifiedFile {
  json: string;
  staleReport: DevSnapshot['verify'];
}

/** The tick to jump to: the first mismatch of the report produced for `file`, if it re-simulated. */
function seekTickOf(snap: DevSnapshot, file: VerifiedFile | null): number | null {
  const report = snap.verify;
  if (!file || !report || report === file.staleReport || report.ok) return null;
  // Rejections (other track, profile or physics) report tick 0: there is nothing to re-simulate.
  const tick = report.firstMismatchTick;
  return tick !== null && tick > 0 ? tick : null;
}

/** Remembers the last verified file and re-simulates it up to its first mismatch (AR28). */
function useReplaySeek(snap: DevSnapshot, actions: DevActions) {
  const [file, setFile] = useState<VerifiedFile | null>(null);
  const [seekError, setSeekError] = useState<string | null>(null);
  const seekTick = seekTickOf(snap, file);
  const seek = () => {
    if (file === null || seekTick === null) return;
    if (file.json.length > DEV_SEEK_MAX_CHARS || seekTick > DEV_SEEK_MAX_TICKS) {
      setSeekError('El replay es demasiado grande para re-simularlo en el laboratorio.');
      return;
    }
    actions.seekReplay(file.json, seekTick);
  };
  const verifyFile = (json: string) => {
    setFile({ json, staleReport: snap.verify });
    setSeekError(null);
    actions.verifyReplayFile(json);
  };
  const verifyCurrent = () => {
    setFile(null);
    actions.verifyCurrentSession();
  };
  return { seekTick, seekError, seek, verifyFile, verifyCurrent };
}

/** Header and tab strip of the panel. */
function PanelHead({ track, snap, tab, onTab }: { track: DevTrack; snap: DevSnapshot; tab: Tab; onTab: (t: Tab) => void }) {
  const km = snap.ready ? `${(snap.ready.trackLength / 1000).toFixed(2)} km` : '…';
  return (
    <>
      <header className="panel-head">
        <h1>Laboratorio de manejo</h1>
        <p>{`${track.name} · ${km}`}</p>
      </header>
      <nav className="tabs" role="tablist">
        {TABS.map(([id, label]) => (
          <button key={id} role="tab" aria-selected={tab === id} onClick={() => onTab(id)}>
            {label}
          </button>
        ))}
      </nav>
    </>
  );
}

type TabProps = Omit<DevPanelProps, 'store' | 'toolbar'> & { snap: DevSnapshot };

/** Session tab, wired to the replay seek state. */
function SessionTab({ snap, actions, track, tracks, seek }: TabProps & Pick<PanelState, 'seek'>) {
  return (
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
      onVerifyCurrent={seek.verifyCurrent}
      onVerifyFile={seek.verifyFile}
      onRestart={actions.restartSession}
      seekTick={seek.seekTick}
      onSeek={seek.seek}
      seekError={seek.seekError}
    />
  );
}

/** State of the panel that must outlive hiding it (the renderer keeps overlays on while hidden). */
interface PanelState {
  tab: Tab;
  overlays: Record<OverlayKind, boolean>;
  setOverlays: (next: Record<OverlayKind, boolean>) => void;
  recordGolden: (name: string) => void;
  seek: ReturnType<typeof useReplaySeek>;
}

/** Body of the selected tab. */
function PanelBody(p: TabProps & PanelState) {
  const { snap, actions, overlays, setOverlays, recordGolden, seek } = p;
  return (
    <div className="panel-body">
      {p.tab === 'ajustes' && snap.ready && snap.appliedProfile && (
        <TuningPanel schema={snap.ready.schema} applied={snap.appliedProfile} onApply={actions.applyProfile} />
      )}
      {p.tab === 'telemetria' && <TelemetryPanel snap={snap} />}
      {p.tab === 'depuracion' && (
        <DebugPanel
          snap={snap}
          actions={actions}
          debug={p.debug}
          overlays={overlays}
          onOverlays={setOverlays}
          onRecordGolden={recordGolden}
        />
      )}
      {p.tab === 'sesion' && <SessionTab {...p} seek={seek} />}
    </div>
  );
}

/** Lab side panel: Tuning, Telemetry, Session and Debug tabs. Texts are literal Spanish (developer tool). */
export function DevPanel({ store, toolbar, ...rest }: DevPanelProps) {
  const snap = useSyncExternalStore(store.subscribe, store.getSnapshot);
  const [tab, setTab] = useState<Tab>('ajustes');
  const [overlays, setOverlays] = useState(NO_OVERLAYS);
  const recordGolden = useReplayDownloads(rest.actions, rest.events, rest.track);
  const seek = useReplaySeek(snap, rest.actions);
  const state: PanelState = { tab, overlays, setOverlays, recordGolden, seek };
  return (
    <>
      <PanelToggle open={snap.panelOpen} actions={rest.actions} toolbar={toolbar} />
      {snap.panelOpen && (
        <aside className="panel" aria-label="Laboratorio">
          <PanelHead track={rest.track} snap={snap} tab={tab} onTab={setTab} />
          <PanelBody {...rest} {...state} snap={snap} />
        </aside>
      )}
    </>
  );
}
