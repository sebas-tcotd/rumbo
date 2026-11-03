// Verifica los replays dorados con el binario WASM, en Node.
// Los replays dorados se generan en nativo (cargo test); si el WASM los reproduce
// con el mismo hash, nativo y WASM son deterministas entre sí.
//
// Uso: npm run wasm && node scripts/verify-golden.mjs
import { readFileSync, readdirSync } from 'node:fs';
import { dirname, join } from 'node:path';
import { fileURLToPath, pathToFileURL } from 'node:url';

const root = join(dirname(fileURLToPath(import.meta.url)), '..');
const pkg = join(root, 'crates/sim-wasm/pkg');
const { default: init, verify_replay, physics_version } = await import(pathToFileURL(join(pkg, 'sim_wasm.js')).href);
await init({ module_or_path: readFileSync(join(pkg, 'sim_wasm_bg.wasm')) });

const track = readFileSync(join(root, 'assets/tracks/test-loop.json'), 'utf8');
const goldenDir = join(root, 'crates/sim-core/tests/golden');
let failed = 0;
for (const file of readdirSync(goldenDir).filter((f) => f.endsWith('.json'))) {
  const report = JSON.parse(verify_replay(track, readFileSync(join(goldenDir, file), 'utf8')));
  const mark = report.ok ? 'OK ' : 'FALLA';
  console.log(`${mark} ${file}: ${report.message} (ticks ${report.ticks}, final ${report.actualFinal})`);
  if (!report.ok) failed++;
}
console.log(`física ${physics_version()} · ${failed ? `${failed} con divergencias` : 'todos idénticos a nativo'}`);
process.exit(failed ? 1 : 0);
