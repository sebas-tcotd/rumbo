import type { LabSnapshot } from '../sim/client';
import { formatLap } from '../sim/state';

interface Props {
  snap: LabSnapshot;
  ghostTicksAt: (s: number) => number | null;
  autopilot: boolean;
  paused: boolean;
}

/** Indicador del ángulo de derrape: aguja actual y banda objetivo del perfil. */
function SlipGauge({ slip, target, exit, drifting }: { slip: number; target: number; exit: number; drifting: boolean }) {
  const R = 64;
  const cx = 76;
  const cy = 74;
  const max = 60;
  const ang = (deg: number) => (Math.max(-max, Math.min(max, deg)) / max) * (Math.PI / 2);
  const pt = (deg: number, r = R) => {
    const a = ang(deg);
    return [cx + Math.sin(a) * r, cy - Math.cos(a) * r];
  };
  const arc = (from: number, to: number, r = R) => {
    const [x0, y0] = pt(from, r);
    const [x1, y1] = pt(to, r);
    const large = Math.abs(ang(to) - ang(from)) > Math.PI ? 1 : 0;
    return `M${x0} ${y0} A${r} ${r} 0 ${large} 1 ${x1} ${y1}`;
  };
  // El ángulo es negativo derrapando a la derecha; la banda se dibuja a ambos lados.
  const [nx, ny] = pt(-slip, R - 6);
  return (
    <svg className="gauge" viewBox="0 0 152 86" role="img" aria-label={`Ángulo de derrape ${Math.abs(slip).toFixed(0)} grados`}>
      <path d={arc(-max, max)} className="gauge-track" />
      <path d={arc(target * 0.7, target * 1.15)} className="gauge-band" />
      <path d={arc(-target * 1.15, -target * 0.7)} className="gauge-band" />
      {[-exit, exit].map((e) => {
        const [x0, y0] = pt(e, R - 9);
        const [x1, y1] = pt(e, R + 3);
        return <line key={e} x1={x0} y1={y0} x2={x1} y2={y1} className="gauge-tick" />;
      })}
      <line x1={cx} y1={cy} x2={nx} y2={ny} className={drifting ? 'gauge-needle on' : 'gauge-needle'} />
      <circle cx={cx} cy={cy} r={4} className="gauge-hub" />
    </svg>
  );
}

export function Hud({ snap, ghostTicksAt, autopilot, paused }: Props) {
  const { state, index: I, ready, appliedProfile } = snap;
  if (!state || !I || !ready) return null;
  const s = state.curr;
  const hz = ready.tickHz;
  const speed = Math.abs(s[I.speedKmh]);
  const slip = s[I.slipDeg];
  const drifting = s[I.drift] > 0.5;
  const started = s[I.nextCheckpoint] >= 1;
  const sprint = !ready.track.closed;
  const idle = sprint
    ? s[I.lap] > 0
      ? 'Llegada · T para otro intento'
      : 'Cruza la salida para empezar'
    : 'Cruza la meta para empezar';
  const params = appliedProfile?.params ?? {};
  const ghostTicks = started ? ghostTicksAt(s[I.progressS]) : null;
  const delta = ghostTicks !== null ? (s[I.lapTicks] - ghostTicks) / hz : null;

  return (
    <div className="hud" aria-live="off">
      <section className="hud-timing">
        <div className="lap-now">{started ? formatLap(s[I.lapTicks], hz) : idle}</div>
        <dl className="lap-meta">
          <div>
            <dt>{sprint ? 'Intento' : 'Vuelta'}</dt>
            <dd>{s[I.lap] + (started ? 1 : 0)}</dd>
          </div>
          <div>
            <dt>Última</dt>
            <dd>{formatLap(s[I.lastLapTicks], hz)}</dd>
          </div>
          <div>
            <dt>Mejor</dt>
            <dd className="best">{formatLap(s[I.bestLapTicks], hz)}</dd>
          </div>
          {delta !== null && (
            <div>
              <dt>Fantasma</dt>
              <dd className={delta <= 0 ? 'ahead' : 'behind'}>
                {delta <= 0 ? '−' : '+'}
                {Math.abs(delta).toFixed(2)}
              </dd>
            </div>
          )}
        </dl>
        <div className="badges">
          {autopilot && <span className="badge">Piloto automático</span>}
          {paused && <span className="badge">En pausa</span>}
          {s[I.invalid] > 0.5 && <span className="badge warn">La física produjo un valor inválido en esta sesión</span>}
        </div>
      </section>

      <section className="hud-drive">
        <div className="speed">
          <span className="speed-value">{speed.toFixed(0)}</span>
          <span className="speed-unit">km/h</span>
        </div>
        <div className="slip">
          <SlipGauge
            slip={slip}
            target={params.drift_target_angle_deg ?? 24}
            exit={params.drift_exit_angle_deg ?? 6}
            drifting={drifting}
          />
          <div className={drifting ? 'slip-label on' : 'slip-label'}>
            {drifting ? 'Derrape' : 'Deslizamiento'} {Math.abs(slip).toFixed(0)}°
          </div>
        </div>
        <div className="pedals" aria-hidden>
          <span style={{ ['--v' as string]: s[I.throttle] }} className="pedal throttle" />
          <span style={{ ['--v' as string]: s[I.brake] }} className="pedal brake" />
        </div>
      </section>
    </div>
  );
}
