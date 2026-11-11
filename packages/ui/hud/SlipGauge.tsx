import { useTranslation } from 'react-i18next';

const R = 64;
const CX = 76;
const CY = 74;
const MAX = 60;

const ang = (deg: number) => (Math.max(-MAX, Math.min(MAX, deg)) / MAX) * (Math.PI / 2);
const pt = (deg: number, r = R) => {
  const a = ang(deg);
  return [CX + Math.sin(a) * r, CY - Math.cos(a) * r];
};
const arc = (from: number, to: number, r = R) => {
  const [x0, y0] = pt(from, r);
  const [x1, y1] = pt(to, r);
  const large = Math.abs(ang(to) - ang(from)) > Math.PI ? 1 : 0;
  return `M${x0} ${y0} A${r} ${r} 0 ${large} 1 ${x1} ${y1}`;
};

/** Props of {@link SlipGauge}: current slip angle and the profile's drift band (degrees). */
export interface SlipGaugeProps {
  slip: number;
  target: number;
  exit: number;
  drifting: boolean;
}

/** Drift angle indicator: current needle and the profile's target band. */
export function SlipGauge({ slip, target, exit, drifting }: SlipGaugeProps) {
  const { t } = useTranslation();
  // The angle is negative when drifting to the right; the band is drawn on both sides.
  const [nx, ny] = pt(-slip, R - 6);
  return (
    <svg
      className="gauge"
      viewBox="0 0 152 86"
      role="img"
      aria-label={t('hud.slipGaugeLabel', { angle: Math.abs(slip).toFixed(0) })}
    >
      <path d={arc(-MAX, MAX)} className="gauge-track" />
      <path d={arc(target * 0.7, target * 1.15)} className="gauge-band" />
      <path d={arc(-target * 1.15, -target * 0.7)} className="gauge-band" />
      {[-exit, exit].map((e) => {
        const [x0, y0] = pt(e, R - 9);
        const [x1, y1] = pt(e, R + 3);
        return <line key={e} x1={x0} y1={y0} x2={x1} y2={y1} className="gauge-tick" />;
      })}
      <line x1={CX} y1={CY} x2={nx} y2={ny} className={drifting ? 'gauge-needle on' : 'gauge-needle'} />
      <circle cx={CX} cy={CY} r={4} className="gauge-hub" />
    </svg>
  );
}
