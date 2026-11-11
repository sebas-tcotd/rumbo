export { buildIndex, STATE_FIELDS, type StateField, type StateIndex } from '@rumbo/contracts';

export const SURFACE_NAMES = ['Asfalto', 'Piano', 'Piano', 'Banquina', 'Muro', 'Meta'];

export function formatLap(ticks: number, hz: number): string {
  if (ticks < 0 || !Number.isFinite(ticks)) return '—';
  const total = ticks / hz;
  const m = Math.floor(total / 60);
  const s = total - m * 60;
  return `${m}:${s.toFixed(3).padStart(6, '0')}`;
}
