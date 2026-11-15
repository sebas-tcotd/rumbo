// Casos: cada replay dorado (`rumbo-replay/2`, `<pista>.<caso>.bin`) y la pista con la que se grabó.
import { readdirSync } from 'node:fs';
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

/**
 * The golden fixtures found on disk. Since story 0.10 they are binary `rumbo-replay/2`: the track id is the
 * file name prefix, and ticks, control hashes and the final hash come from the fixture's own summary, which
 * Rust decodes into the native reference (`summary` of each case, see crates/sim-wasm/examples/
 * trace_reference.rs). JavaScript never decodes postcard (D15, D16). A fixture without a summary keeps
 * `ticks: NaN` so the evaluation reports it instead of skipping it.
 * @param root      repository root.
 * @param reference parsed `reference/native.json` (or undefined when unreadable).
 */
export function listCases(root, reference) {
  const dir = join(root, 'crates/sim-core/tests/golden');
  const summaries = new Map((reference?.cases ?? []).map((c) => [c.name, c.summary]));
  return readdirSync(dir)
    .filter((f) => f.endsWith('.bin'))
    .sort()
    .map((f) => {
      const name = f.replace(/\.bin$/, '');
      const summary = summaries.get(name);
      return {
        name,
        replay: `/crates/sim-core/tests/golden/${f}`,
        track: `/assets/tracks/${name.split('.')[0]}.json`,
        expected: summary?.finalHash,
        ticks: summary?.ticks ?? Number.NaN,
        checks: summary?.checks ?? [],
      };
    });
}
