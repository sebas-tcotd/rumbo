import { useCallback, useEffect, useRef, useState, useSyncExternalStore } from 'react';
import defaultProfile from '@assets/profiles/hatch-arcade.json';
import { SimClient, type LabSnapshot } from './sim/client';
import type { ProfileFile } from './sim/protocol';
import { LabRenderer, type CameraMode } from './render/LabRenderer';
import { InputSampler, type DriveInput } from './input/input';
import { Hud } from './hud/Hud';
import { TuningPanel } from './debug/TuningPanel';
import { TelemetryPanel } from './debug/TelemetryPanel';
import { SessionPanel } from './debug/SessionPanel';
import { download } from './debug/download';
import { selectedTrack, switchTrack, TRACKS } from './sim/tracks';

const TRACK = selectedTrack();

const CAMERAS: CameraMode[] = ['persecucion', 'cercana', 'aerea'];
const CAMERA_NAMES: Record<CameraMode, string> = { persecucion: 'persecución', cercana: 'cercana', aerea: 'aérea' };
type Tab = 'ajustes' | 'telemetria' | 'sesion';

const EMPTY: LabSnapshot = {
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
const noopSubscribe = () => () => {};

export function App() {
  const viewport = useRef<HTMLDivElement>(null);
  const [client, setClient] = useState<SimClient | null>(null);
  const [renderer, setRenderer] = useState<LabRenderer | null>(null);
  const [autopilot, setAutopilot] = useState(false);
  const [paused, setPaused] = useState(false);
  const [camera, setCamera] = useState<CameraMode>('persecucion');
  const [ghostOn, setGhostOn] = useState(true);
  const [panelOpen, setPanelOpen] = useState(true);
  const [tab, setTab] = useState<Tab>('ajustes');
  const verifyAfterExport = useRef(false);

  const snap = useSyncExternalStore(client?.subscribe ?? noopSubscribe, client?.getSnapshot ?? (() => EMPTY));

  // Arranque: worker, render y entradas. Se desmonta limpio (también en StrictMode).
  useEffect(() => {
    const el = viewport.current;
    if (!el) return;
    const c = new SimClient();
    const r = new LabRenderer(el);
    let disposed = false;
    let lastSent = '';
    const keyActions = new Map<string, () => void>();
    const sampler = new InputSampler((code) => keyActions.get(code)?.());

    c.onReady = (ready, index) => r.build(ready, index);
    c.onState = (m) => r.setState(m);
    c.onGhost = (g) => r.setGhost(g);
    c.onReplay = (json, ticks) => {
      if (verifyAfterExport.current) {
        verifyAfterExport.current = false;
        c.send({ type: 'verifyReplay', json });
      } else {
        download(`replay-${ticks}-ticks.json`, json);
      }
    };
    r.onFrame = () => {
      const i: DriveInput = sampler.sample();
      const key = `${i.steer.toFixed(3)}|${i.throttle.toFixed(3)}|${i.brake.toFixed(3)}|${i.handbrake}`;
      if (key !== lastSent) {
        lastSent = key;
        c.send({ type: 'input', ...i });
      }
    };

    const profile = defaultProfile as ProfileFile;
    c.init(TRACK.json, profile);
    r.setSuspensionRest(profile.params.suspension_rest_m);
    r.start().then(() => {
      if (!disposed) setRenderer(r);
    });
    setClient(c);

    keyActions.set('KeyR', () => c.send({ type: 'reset' }));
    keyActions.set('KeyT', () => c.send({ type: 'restart' }));
    keyActions.set('KeyP', () => setAutopilot((v) => !v));
    keyActions.set('KeyC', () => setCamera((v) => CAMERAS[(CAMERAS.indexOf(v) + 1) % CAMERAS.length]));
    keyActions.set('KeyG', () => setGhostOn((v) => !v));
    keyActions.set('KeyH', () => setPanelOpen((v) => !v));
    keyActions.set('Escape', () => setPaused((v) => !v));

    return () => {
      disposed = true;
      sampler.dispose();
      r.dispose();
      c.dispose();
      setClient(null);
      setRenderer(null);
    };
  }, []);

  useEffect(() => client?.send({ type: 'autopilot', on: autopilot }), [client, autopilot]);
  useEffect(() => client?.send({ type: 'pause', on: paused }), [client, paused]);
  useEffect(() => {
    if (renderer) renderer.cameraMode = camera;
  }, [renderer, camera]);
  useEffect(() => {
    if (renderer) renderer.showGhost = ghostOn;
  }, [renderer, ghostOn]);

  const applyProfile = useCallback(
    (p: ProfileFile) => {
      client?.newSession(p);
      renderer?.setSuspensionRest(p.params.suspension_rest_m);
    },
    [client, renderer],
  );

  const ghostTicksAt = useCallback((s: number) => renderer?.ghostTicksAt(s) ?? null, [renderer]);
  const loading = !snap.ready || !renderer;

  return (
    <div className={panelOpen ? 'lab' : 'lab panel-hidden'}>
      <div className="viewport" ref={viewport}>
        {!loading && <Hud snap={snap} ghostTicksAt={ghostTicksAt} autopilot={autopilot} paused={paused} />}
        {loading && !snap.error && <div className="loading">Cargando la simulación…</div>}
        {snap.error && (
          <div className="fatal" role="alert">
            <strong>La simulación no pudo iniciar.</strong>
            <p>{snap.error}</p>
            <p>Si falta el paquete WASM, ejecuta <code>npm run wasm</code> en apps/web y recarga.</p>
          </div>
        )}
        <button className="panel-toggle" onClick={() => setPanelOpen((v) => !v)} aria-expanded={panelOpen}>
          {panelOpen ? 'Ocultar panel' : 'Mostrar panel'}
        </button>
      </div>

      {panelOpen && (
        <aside className="panel" aria-label="Laboratorio">
          <header className="panel-head">
            <h1>Laboratorio de manejo</h1>
            <p>{`${TRACK.name} · ${snap.ready ? (snap.ready.trackLength / 1000).toFixed(2) + ' km' : '…'}`}</p>
          </header>
          <nav className="tabs" role="tablist">
            {(
              [
                ['ajustes', 'Ajustes'],
                ['telemetria', 'Telemetría'],
                ['sesion', 'Sesión'],
              ] as [Tab, string][]
            ).map(([id, label]) => (
              <button key={id} role="tab" aria-selected={tab === id} onClick={() => setTab(id)}>
                {label}
              </button>
            ))}
          </nav>
          <div className="panel-body">
            {tab === 'ajustes' && snap.ready && snap.appliedProfile && (
              <TuningPanel
                schema={snap.ready.schema}
                applied={snap.appliedProfile}
                onApply={applyProfile}
              />
            )}
            {tab === 'telemetria' && <TelemetryPanel snap={snap} backend={renderer?.backend ?? '—'} />}
            {tab === 'sesion' && (
              <SessionPanel
                snap={snap}
                track={TRACK}
                tracks={TRACKS}
                onTrack={switchTrack}
                autopilot={autopilot}
                onAutopilot={setAutopilot}
                ghostOn={ghostOn}
                onGhost={setGhostOn}
                camera={CAMERA_NAMES[camera]}
                onCamera={() => setCamera((v) => CAMERAS[(CAMERAS.indexOf(v) + 1) % CAMERAS.length])}
                onExport={() => client?.send({ type: 'exportReplay' })}
                onVerifyCurrent={() => {
                  verifyAfterExport.current = true;
                  client?.send({ type: 'exportReplay' });
                }}
                onVerifyFile={(json) => client?.send({ type: 'verifyReplay', json })}
                onRestart={() => snap.appliedProfile && applyProfile(snap.appliedProfile)}
              />
            )}
          </div>
        </aside>
      )}
    </div>
  );
}
