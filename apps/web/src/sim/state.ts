// Acceso al estado por nombre. Los índices vienen de `state_layout()` en Rust;
// la lista de abajo solo da seguridad de tipos y se comprueba al iniciar.

export const STATE_FIELDS = [
  'tick', 'px', 'py', 'pz', 'qx', 'qy', 'qz', 'qw', 'vx', 'vy', 'vz',
  'speedKmh', 'slipDeg', 'steerDeg', 'steerInput', 'drift',
  'contactFL', 'contactFR', 'contactRL', 'contactRR',
  'compFL', 'compFR', 'compRL', 'compRR',
  'surfFL', 'surfFR', 'surfRL', 'surfRR',
  'lap', 'lapTicks', 'lastLapTicks', 'bestLapTicks', 'nextCheckpoint',
  'progressS', 'trackLength', 'yawRate', 'latG', 'throttle', 'brake', 'invalid', 'sampleIndex',
] as const;

export type StateField = (typeof STATE_FIELDS)[number];
export type StateIndex = Record<StateField, number>;

export function buildIndex(layout: string[]): StateIndex {
  const idx = {} as StateIndex;
  for (const f of STATE_FIELDS) {
    const i = layout.indexOf(f);
    if (i < 0) throw new Error(`El estado de la simulación no incluye "${f}". Recompila el paquete WASM.`);
    idx[f] = i;
  }
  return idx;
}

export const SURFACE_NAMES = ['Asfalto', 'Piano', 'Piano', 'Banquina', 'Muro', 'Meta'];

export function formatLap(ticks: number, hz: number): string {
  if (ticks < 0 || !Number.isFinite(ticks)) return '—';
  const total = ticks / hz;
  const m = Math.floor(total / 60);
  const s = total - m * 60;
  return `${m}:${s.toFixed(3).padStart(6, '0')}`;
}
