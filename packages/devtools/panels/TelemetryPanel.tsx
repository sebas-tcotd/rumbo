import { useEffect, useRef } from 'react';
import type { StateIndex } from '@rumbo/contracts';
import type { DevSnapshot } from '../dev-snapshot.ts';

/** Surface labels in the order of `Surface` in sim-core. */
const SURFACE_NAMES = ['Asfalto', 'Piano', 'Piano', 'Banquina', 'Muro', 'Meta'];

const SECONDS = 8;
const RATE = 15; // samples per second (UI refresh rate of the lab store)

interface Series {
  label: string;
  unit: string;
  min: number;
  max: number;
  zero?: boolean;
  read: (s: Float32Array, I: StateIndex) => number;
}

const SERIES: Series[] = [
  { label: 'Velocidad', unit: 'km/h', min: 0, max: 240, read: (s, I) => Math.abs(s[I.speedKmh]) },
  { label: 'Ángulo de deslizamiento', unit: '°', min: -45, max: 45, zero: true, read: (s, I) => -s[I.slipDeg] },
  { label: 'Aceleración lateral', unit: 'g', min: -2, max: 2, zero: true, read: (s, I) => s[I.latG] },
  { label: 'Dirección aplicada', unit: '°', min: -35, max: 35, zero: true, read: (s, I) => s[I.steerDeg] },
];

function Sparkline({ values, series }: { values: number[]; series: Series }) {
  const W = 300;
  const H = 46;
  const n = SECONDS * RATE;
  const y = (v: number) => H - ((Math.max(series.min, Math.min(series.max, v)) - series.min) / (series.max - series.min)) * H;
  const d = values.map((v, i) => `${i ? 'L' : 'M'}${(i / (n - 1)) * W} ${y(v).toFixed(1)}`).join('');
  const last = values.at(-1) ?? 0;
  return (
    <figure className="spark">
      <figcaption>
        <span>{series.label}</span>
        <strong>
          {last.toFixed(series.unit === 'g' ? 2 : 0)} {series.unit}
        </strong>
      </figcaption>
      <svg viewBox={`0 0 ${W} ${H}`} preserveAspectRatio="none" aria-hidden>
        {series.zero && <line x1={0} x2={W} y1={y(0)} y2={y(0)} className="spark-zero" />}
        <path d={d} className="spark-line" />
      </svg>
    </figure>
  );
}

/** Sparklines, wheel contacts and sim/render stats from the ~15 Hz state capture. */
export function TelemetryPanel({ snap }: { snap: DevSnapshot }) {
  const hist = useRef<number[][]>(SERIES.map(() => []));
  const { state, index: I } = snap;

  useEffect(() => {
    if (!state || !I) return;
    SERIES.forEach((ser, k) => {
      const h = hist.current[k];
      h.push(ser.read(state.curr, I));
      if (h.length > SECONDS * RATE) h.shift();
    });
  }, [state, I]);

  if (!state || !I) return <p className="note">Esperando a la simulación…</p>;
  const s = state.curr;
  const wheels = ['FL', 'FR', 'RL', 'RR'] as const;
  const names = { FL: 'Del. izq.', FR: 'Del. der.', RL: 'Tras. izq.', RR: 'Tras. der.' };
  const rest = snap.appliedProfile?.params.suspension_rest_m ?? 0.3;

  return (
    <div className="telemetry">
      {SERIES.map((ser, k) => (
        <Sparkline key={ser.label} series={ser} values={hist.current[k]} />
      ))}

      <table className="wheels">
        <caption>Ruedas</caption>
        <thead>
          <tr>
            <th scope="col">Rueda</th>
            <th scope="col">Compresión</th>
            <th scope="col">Superficie</th>
          </tr>
        </thead>
        <tbody>
          {wheels.map((w) => {
            const contact = s[I[`contact${w}`]] > 0.5;
            const comp = s[I[`comp${w}`]];
            return (
              <tr key={w} className={contact ? '' : 'air'}>
                <th scope="row">{names[w]}</th>
                <td>
                  <span className="bar" style={{ ['--v' as string]: Math.min(1, comp / rest) }} />
                  {contact ? `${(comp * 100).toFixed(1)} cm` : 'en el aire'}
                </td>
                <td>{contact ? SURFACE_NAMES[s[I[`surf${w}`]]] ?? '—' : '—'}</td>
              </tr>
            );
          })}
        </tbody>
      </table>

      <dl className="stats">
        <div>
          <dt>Frecuencia de simulación</dt>
          <dd>{state.simHz} Hz</dd>
        </div>
        <div>
          <dt>Tiempo por paso físico</dt>
          <dd>{(state.stepMs * 1000).toFixed(0)} µs</dd>
        </div>
        <div>
          <dt>Tick</dt>
          <dd>{s[I.tick].toFixed(0)}</dd>
        </div>
        <div>
          <dt>Velocidad de giro</dt>
          <dd>{s[I.yawRate].toFixed(2)} rad/s</dd>
        </div>
        <div>
          <dt>Progreso</dt>
          <dd>
            {s[I.progressS].toFixed(0)} / {s[I.trackLength].toFixed(0)} m
          </dd>
        </div>
        <div>
          <dt>Render</dt>
          <dd>{snap.backend}</dd>
        </div>
      </dl>
    </div>
  );
}
