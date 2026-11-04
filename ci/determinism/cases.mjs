// Casos: cada replay dorado y la pista con la que se grabó (por su id).
import { readFileSync, readdirSync } from 'node:fs';
import { join } from 'node:path';

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
      };
    });
}
