import { ghostTicksAt, type StateIndex } from '@rumbo/contracts';
import { useTranslation } from 'react-i18next';
import type { UiSnapshot } from '../ui-snapshot.ts';
import { formatLap } from './format-lap.ts';
import { SlipGauge } from './SlipGauge.tsx';

/** Props of {@link Hud}. */
export interface HudProps {
  snap: UiSnapshot;
}

interface PartProps {
  snap: UiSnapshot;
  s: Float32Array;
  I: StateIndex;
}

function idleKey(sprint: boolean, s: Float32Array, I: StateIndex): string {
  if (!sprint) return 'hud.idleCircuit';
  return s[I.lap] > 0 ? 'hud.idleSprintFinished' : 'hud.idleSprintStart';
}

function GhostDelta({ delta }: { delta: number }) {
  const { t } = useTranslation();
  return (
    <div>
      <dt>{t('hud.ghost')}</dt>
      <dd className={delta <= 0 ? 'ahead' : 'behind'}>
        {delta <= 0 ? '−' : '+'}
        {Math.abs(delta).toFixed(2)}
      </dd>
    </div>
  );
}

function Badges({ snap, s, I }: PartProps) {
  const { t } = useTranslation();
  return (
    <div className="badges">
      {snap.autopilot && <span className="badge">{t('hud.autopilot')}</span>}
      {snap.paused && <span className="badge">{t('hud.paused')}</span>}
      {s[I.invalid] > 0.5 && <span className="badge warn">{t('hud.invalidPhysics')}</span>}
    </div>
  );
}

/** Timing block: current lap, lap counter, last/best and the ghost delta. */
function Timing({ snap, s, I }: PartProps) {
  const { t } = useTranslation();
  const ready = snap.ready;
  if (!ready) return null;
  const hz = ready.tickHz;
  const started = s[I.nextCheckpoint] >= 1;
  const sprint = !ready.track.closed;
  const ghostTicks = started ? ghostTicksAt(snap.ghost, s[I.progressS]) : null;
  const delta = ghostTicks !== null ? (s[I.lapTicks] - ghostTicks) / hz : null;
  return (
    <section className="hud-timing">
      <div className="lap-now">{started ? formatLap(s[I.lapTicks], hz) : t(idleKey(sprint, s, I))}</div>
      <dl className="lap-meta">
        <div>
          <dt>{t(sprint ? 'hud.attempt' : 'hud.lap')}</dt>
          <dd>{s[I.lap] + (started ? 1 : 0)}</dd>
        </div>
        <div>
          <dt>{t('hud.lastLap')}</dt>
          <dd>{formatLap(s[I.lastLapTicks], hz)}</dd>
        </div>
        <div>
          <dt>{t('hud.bestLap')}</dt>
          <dd className="best">{formatLap(s[I.bestLapTicks], hz)}</dd>
        </div>
        {delta !== null && <GhostDelta delta={delta} />}
      </dl>
      <Badges snap={snap} s={s} I={I} />
    </section>
  );
}

/** Speed, drift gauge and pedals. */
function Drive({ snap, s, I }: PartProps) {
  const { t } = useTranslation();
  // Rapier publishes this velocity after resolving the tick's forces and contacts.
  const speed = Math.hypot(s[I.vx], s[I.vy], s[I.vz]) * 3.6;
  const slip = s[I.slipDeg];
  const drifting = s[I.drift] > 0.5;
  const params = snap.appliedProfile?.params ?? {};
  return (
    <section className="hud-drive">
      <div className="speed">
        <span className="speed-value">{speed.toFixed(0)}</span>
        <span className="speed-unit">{t('hud.speedUnit')}</span>
      </div>
      <div className="slip">
        <SlipGauge
          slip={slip}
          target={params.drift_target_angle_deg ?? 24}
          exit={params.drift_exit_angle_deg ?? 6}
          drifting={drifting}
        />
        <div className={drifting ? 'slip-label on' : 'slip-label'}>
          {t(drifting ? 'hud.drift' : 'hud.slip')} {Math.abs(slip).toFixed(0)}°
        </div>
      </div>
      <div className="pedals" aria-hidden>
        <span style={{ ['--v' as string]: s[I.throttle] }} className="pedal throttle" />
        <span style={{ ['--v' as string]: s[I.brake] }} className="pedal brake" />
      </div>
    </section>
  );
}

/** Race HUD. Reads the ~15 Hz state capture from the host store, never per-frame state. */
export function Hud({ snap }: HudProps) {
  const { state, index: I, ready } = snap;
  if (!state || !I || !ready) return null;
  return (
    <div className="hud" aria-live="off">
      <Timing snap={snap} s={state.curr} I={I} />
      <Drive snap={snap} s={state.curr} I={I} />
    </div>
  );
}
