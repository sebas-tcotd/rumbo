import { useRef } from 'react';
import type { LabSnapshot } from '../sim/client';
import { formatLap } from '../sim/state';
import type { TrackEntry } from '../sim/tracks';

interface Props {
  snap: LabSnapshot;
  track: TrackEntry;
  tracks: TrackEntry[];
  onTrack: (id: string) => void;
  autopilot: boolean;
  onAutopilot: (on: boolean) => void;
  ghostOn: boolean;
  onGhost: (on: boolean) => void;
  camera: string;
  onCamera: () => void;
  onExport: () => void;
  onVerifyCurrent: () => void;
  onVerifyFile: (json: string) => void;
  onRestart: () => void;
}

const KEYS: [string, string][] = [
  ['W / ↑', 'Acelerar'],
  ['S / ↓', 'Frenar; marcha atrás si estás detenido'],
  ['A D / ← →', 'Dirección'],
  ['Espacio + dirección', 'Derrape (sobre la velocidad mínima de derrape)'],
  ['S + dirección', 'Frenada en curva, sin derrape'],
  ['R', 'Recolocar el auto sobre la pista'],
  ['T', 'Volver a la salida y empezar otro intento'],
  ['P', 'Piloto automático'],
  ['C', 'Cambiar cámara'],
  ['G', 'Mostrar u ocultar el fantasma'],
  ['H', 'Mostrar u ocultar el panel'],
  ['Esc', 'Pausa'],
];

export function SessionPanel(p: Props) {
  const fileRef = useRef<HTMLInputElement>(null);
  const { ready, hashes, laps, verify } = p.snap;
  const hz = ready?.tickHz ?? 120;

  const t = p.track;
  const warnings = t.diagnostics.filter((d) => d.severity !== 'info');
  const statusText = { VALID: 'Válida', DEGRADED: 'Degradada', UNSUPPORTED: 'No soportada' } as const;
  return (
    <div className="session">
      <section>
        <h3>Pista</h3>
        <label className="field">
          <span>Pista activa</span>
          <select value={t.id} onChange={(e) => p.onTrack(e.target.value)}>
            {p.tracks.map((x) => (
              <option key={x.id} value={x.id}>
                {x.name}
              </option>
            ))}
          </select>
        </label>
        {t.status && (
          <>
            <p className={`track-status ${t.status.toLowerCase()}`}>
              {statusText[t.status]}: generada desde OpenStreetMap y Copernicus GLO-30
              {t.stats && ` · ${(t.stats.lengthM / 1000).toFixed(1)} km · sube ${t.stats.climbM.toFixed(0)} m · pendiente máxima ${t.stats.maxGradePct.toFixed(1)} %`}
            </p>
            {warnings.length > 0 && (
              <details className="diagnostics">
                <summary>
                  {warnings.length} {warnings.length === 1 ? 'aviso' : 'avisos'} del compilador
                </summary>
                <ul>
                  {warnings.map((d, i) => (
                    <li key={i}>
                      {d.message}
                      {d.at && (
                        <>
                          {' '}
                          <a href={`https://www.openstreetmap.org/?mlat=${d.at.lat}&mlon=${d.at.lon}#map=17/${d.at.lat}/${d.at.lon}`} target="_blank" rel="noreferrer">
                            Ver en el mapa
                          </a>
                        </>
                      )}
                    </li>
                  ))}
                </ul>
              </details>
            )}
            {t.attribution && <p className="note">{t.attribution}</p>}
          </>
        )}
        {!t.closed && <p className="note">Sprint: el tiempo corre de la salida a la llegada. Con T vuelves a la salida.</p>}
      </section>

      <section>
        <h3>Conducción</h3>
        <label className="toggle">
          <input type="checkbox" checked={p.autopilot} onChange={(e) => p.onAutopilot(e.target.checked)} />
          Piloto automático
        </label>
        <label className="toggle">
          <input type="checkbox" checked={p.ghostOn} onChange={(e) => p.onGhost(e.target.checked)} />
          Fantasma de la mejor vuelta
        </label>
        <div className="row">
          <button onClick={p.onCamera}>Cámara: {p.camera}</button>
          <button onClick={p.onRestart}>Reiniciar sesión</button>
        </div>
      </section>

      <section>
        <h3>Vueltas de esta sesión</h3>
        {laps.length === 0 ? (
          <p className="note">Todavía no hay vueltas completas. La primera empieza al cruzar la meta.</p>
        ) : (
          <ol className="laps">
            {laps.map((l, i) => (
              <li key={i} className={l.best ? 'best' : ''}>
                {formatLap(l.ticks, hz)}
              </li>
            ))}
          </ol>
        )}
      </section>

      <section>
        <h3>Replay y determinismo</h3>
        <p className="note">
          El replay guarda las entradas de cada tick desde que empezó la sesión. Verificarlo vuelve a simular todo desde
          cero y compara los hashes de estado.
        </p>
        <div className="row">
          <button onClick={p.onVerifyCurrent}>Verificar esta sesión</button>
          <button onClick={p.onExport}>Descargar replay</button>
          <button onClick={() => fileRef.current?.click()}>Verificar un archivo</button>
        </div>
        <input
          ref={fileRef}
          type="file"
          accept="application/json"
          hidden
          onChange={async (e) => {
            const f = e.target.files?.[0];
            if (f) p.onVerifyFile(await f.text());
            e.target.value = '';
          }}
        />
        {p.snap.verifyError && <p className="verify bad" role="alert">{p.snap.verifyError}</p>}
        {verify && (
          <p className={verify.ok ? 'verify ok' : 'verify bad'} role="status">
            {verify.ok ? 'Idéntico: ' : 'Divergencia: '}
            {verify.message}. {verify.ticks} ticks, hash final {verify.actualFinal || '—'}.
          </p>
        )}
        <dl className="stats">
          <div>
            <dt>Pista</dt>
            <dd>{hashes?.track ?? '—'}</dd>
          </div>
          <div>
            <dt>Perfil</dt>
            <dd>{hashes?.profile ?? '—'}</dd>
          </div>
          <div>
            <dt>Versión de física</dt>
            <dd>{ready?.physicsVersion ?? '—'}</dd>
          </div>
        </dl>
      </section>

      <section>
        <h3>Controles</h3>
        <p className="note">Con mando: gatillos para acelerar y frenar, stick izquierdo para la dirección, A o RB freno de mano, Y recolocar.</p>
        <table className="keys">
          <tbody>
            {KEYS.map(([k, d]) => (
              <tr key={k}>
                <th scope="row">
                  <kbd>{k}</kbd>
                </th>
                <td>{d}</td>
              </tr>
            ))}
          </tbody>
        </table>
      </section>
    </div>
  );
}
