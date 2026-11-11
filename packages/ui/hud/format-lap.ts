/** Lap time as m:ss.mmm from ticks; an em dash for a missing (negative or non-finite) time. */
export function formatLap(ticks: number, hz: number): string {
  if (ticks < 0 || !Number.isFinite(ticks)) return '—';
  const total = ticks / hz;
  const m = Math.floor(total / 60);
  const s = total - m * 60;
  return `${m}:${s.toFixed(3).padStart(6, '0')}`;
}
