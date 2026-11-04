// Verifica los replays dorados con el binario WASM, en Node.
// Los replays dorados se generan en nativo (cargo test --test golden); si el WASM los
// reproduce con los mismos hashes, nativo y WASM son deterministas entre sí. Para los
// navegadores, ver ci/determinism.
//
// Uso: npm run wasm && node scripts/verify-golden.mjs
import { spawnSync } from 'node:child_process';
import { dirname, join } from 'node:path';
import { fileURLToPath } from 'node:url';

const root = join(dirname(fileURLToPath(import.meta.url)), '..');
const r = spawnSync(process.execPath, [join(root, 'ci/determinism/run.mjs'), '--browser', 'node'], { stdio: 'inherit' });
process.exit(r.status ?? 1);
