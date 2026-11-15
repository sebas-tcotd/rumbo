// Casos: cada replay dorado y la pista con la que se grabó (por su id).
import { readFileSync, readdirSync } from 'node:fs';
import { join } from 'node:path';

// Story 0.9: the eight golden replays, by identity. The matrix is validated against this list, not against
// what each runner happens to discover, so deleting a fixture fails everywhere instead of shrinking the set.
// Adding or renaming a golden is a P1 change (`physics:` commit with a PHYSICS_VERSION bump) and updates this list.
export const REQUIRED_CASES = Object.freeze([
  'costa-verde-sprint.adversarial',
  'costa-verde-sprint.autopilot',
  'san-borja-las-artes.adversarial',
  'san-borja-las-artes.autopilot',
  'san-borja-las-artes.scripted',
  'test-loop.adversarial',
  'test-loop.autopilot',
  'test-loop.scripted',
]);

export function listCases(root) {
  const dir = join(root, 'crates/sim-core/tests/golden');
  return readdirSync(dir)
    .filter((f) => f.endsWith('.json'))
    .sort()
    .map((f) => {
      const manifest = JSON.parse(readFileSync(join(dir, f), 'utf8'));
      return {
        name: f.replace(/\.json$/, ''),
        replay: `/crates/sim-core/tests/golden/${f}`,
        track: `/assets/tracks/${manifest.trackId}.json`,
        expected: manifest.finalStateHash,
        ticks: manifest.ticks,
        checks: manifest.checks ?? [],
      };
    });
}
