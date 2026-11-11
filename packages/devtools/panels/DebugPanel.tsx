import type { OverlayKind, RenderDebugPort } from '@rumbo/contracts';
import { useState } from 'react';
import type { DevActions, DevSnapshot } from '../dev-snapshot.ts';

const OVERLAYS: [OverlayKind, string, string][] = [
  ['axis', 'Eje de la pista', 'Línea central que usa la simulación para medir el avance (s).'],
  ['checkpoints', 'Checkpoints', 'Líneas transversales en cada checkpoint del cronómetro.'],
  ['collision', 'Cajas de colisión', 'Caja del chasis sobre el auto y malla de los muros.'],
];
const CASE_NAME = /^[a-z0-9][a-z0-9-]*$/;

/** Props of {@link DebugPanel}. */
export interface DebugPanelProps {
  snap: DevSnapshot;
  actions: DevActions;
  debug: RenderDebugPort;
  overlays: Record<OverlayKind, boolean>;
  onOverlays: (next: Record<OverlayKind, boolean>) => void;
  /** Exports the current session as `<track>.<caseName>.json` (golden replay format). */
  onRecordGolden: (caseName: string) => void;
}

function Overlays({ debug, overlays, onOverlays }: Pick<DebugPanelProps, 'debug' | 'overlays' | 'onOverlays'>) {
  const toggle = (kind: OverlayKind, on: boolean) => {
    debug.setOverlay(kind, on);
    onOverlays({ ...overlays, [kind]: on });
  };
  return (
    <section>
      <h3>Overlays</h3>
      {OVERLAYS.map(([kind, label, help]) => (
        <div key={kind} className="param">
          <label className="toggle">
            <input type="checkbox" checked={overlays[kind]} onChange={(e) => toggle(kind, e.target.checked)} />
            {label}
          </label>
          <div className="param-help">{help}</div>
        </div>
      ))}
    </section>
  );
}

function Teleport({ snap, actions }: Pick<DebugPanelProps, 'snap' | 'actions'>) {
  const [s, setS] = useState(0);
  const length = snap.ready?.trackLength ?? 0;
  const progress = snap.state && snap.index ? snap.state.curr[snap.index.progressS] : null;
  return (
    <section>
      <h3>Teletransporte</h3>
      <label className="field">
        <span>Distancia s (m, 0 a {length.toFixed(0)})</span>
        <input type="number" min={0} max={length} step={10} value={s} onChange={(e) => setS(Number(e.target.value))} />
      </label>
      <div className="row">
        <button disabled={!snap.ready || !Number.isFinite(s)} onClick={() => actions.teleport(s)}>
          Teletransportar
        </button>
      </div>
      <p className="note">
        Avance actual: {progress === null ? '—' : `${progress.toFixed(0)} m`}. El auto queda en reposo sobre el eje y
        se descarta el intento. La sesión queda alterada: su replay ya no se reproduce.
      </p>
    </section>
  );
}

function RecordGolden({ snap, onRecordGolden }: Pick<DebugPanelProps, 'snap' | 'onRecordGolden'>) {
  const [name, setName] = useState('lab');
  const valid = CASE_NAME.test(name);
  return (
    <section>
      <h3>Grabar dorado</h3>
      <label className="field">
        <span>Caso</span>
        <input type="text" value={name} onChange={(e) => setName(e.target.value.trim())} />
      </label>
      <div className="row">
        <button disabled={!snap.ready || snap.altered || !valid} onClick={() => onRecordGolden(name)}>
          Grabar dorado
        </button>
      </div>
      <p className="note">
        {snap.altered
          ? 'Esta sesión está alterada (teletransporte o salto): no sirve como dorado. Reinicia la sesión.'
          : 'Descarga el replay de esta sesión como <pista>.<caso>.json, en el formato de crates/sim-core/tests/golden/. Sumarlo al set (CASES en golden.rs) es un cambio aparte.'}
      </p>
      {!valid && <p className="error">El caso usa minúsculas, dígitos y guiones.</p>}
    </section>
  );
}

/** AR28 debug tools: overlays, teleport and golden recording. Reaches the renderer only through ports. */
export function DebugPanel(p: DebugPanelProps) {
  return (
    <div className="session">
      <Overlays debug={p.debug} overlays={p.overlays} onOverlays={p.onOverlays} />
      <Teleport snap={p.snap} actions={p.actions} />
      <RecordGolden snap={p.snap} onRecordGolden={p.onRecordGolden} />
    </div>
  );
}
