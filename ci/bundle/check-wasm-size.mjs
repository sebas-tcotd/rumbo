// Story 0.9 (NFR3, D9): the simulation WASM served by the publication build must stay at or under
// 1 500 000 bytes gzip-compressed (level 9, fixed), and it must not carry the S4 harness export.
// Run after `pnpm turbo run build` (which builds apps/web/dist):
//   node ci/bundle/check-wasm-size.mjs [--dist apps/web/dist] [--limit 1500000]
import { readdirSync, readFileSync, existsSync } from 'node:fs';
import { join, relative, resolve } from 'node:path';
import { fileURLToPath } from 'node:url';
import { constants, gzipSync } from 'node:zlib';

const repo = fileURLToPath(new URL('../../', import.meta.url));

/** Inclusive budget, in gzip bytes. */
export const LIMIT = 1_500_000;
/** Fixed compression level, so the measure is reproducible. */
export const GZIP_LEVEL = constants.Z_BEST_COMPRESSION;
/** Exports that only the CI test binary (sim-wasm `--features harness`) may have. */
export const HARNESS_EXPORTS = ['trace_replay'];
/** Vite emits the sim binary as `sim_wasm_bg-<hash>.wasm` (or unhashed in some setups). */
const SIM_WASM = /^sim_wasm_bg(-[\w-]+)?\.wasm$/;

function files(dir) {
  return readdirSync(dir, { withFileTypes: true }).flatMap((e) => (e.isDirectory() ? files(join(dir, e.name)) : [join(dir, e.name)]));
}

/** Gzip size at the fixed level. */
export function gzipSize(bytes) {
  return gzipSync(bytes, { level: GZIP_LEVEL }).length;
}

/**
 * Checks the publication build in `dist`. Returns `{ ok, problems, report }`; report lists each binary
 * with its raw and gzip sizes. Missing dist, no binary, more than one, an unparsable module, a harness
 * export or a size above `limit` are failures.
 */
export function checkWasm(dist, limit = LIMIT) {
  const problems = [];
  const report = [];
  if (!existsSync(dist)) return { ok: false, problems: [`no existe el build de publicación: ${dist}`], report };
  const found = files(dist).filter((f) => SIM_WASM.test(f.split(/[\\/]/).pop()));
  if (found.length === 0) problems.push(`no hay WASM de simulación (sim_wasm_bg*.wasm) en ${dist}`);
  if (found.length > 1) problems.push(`más de un WASM de simulación en ${dist}: ${found.map((f) => relative(dist, f)).join(', ')}`);
  for (const f of found) {
    const bytes = readFileSync(f);
    const gzip = gzipSize(bytes);
    const path = relative(repo, f).replaceAll('\\', '/');
    report.push({ path, raw: bytes.length, gzip, limit });
    if (gzip > limit) problems.push(`${path}: ${gzip} bytes gzip superan el tope de ${limit}`);
    let exports;
    try {
      exports = WebAssembly.Module.exports(new WebAssembly.Module(bytes)).map((e) => e.name);
    } catch (e) {
      problems.push(`${path}: no es un módulo WebAssembly válido (${e.message})`);
      continue;
    }
    const leaked = HARNESS_EXPORTS.filter((n) => exports.includes(n));
    if (leaked.length) problems.push(`${path}: el binario de publicación exporta el arnés (${leaked.join(', ')})`);
  }
  return { ok: problems.length === 0, problems, report };
}

export function markdown({ ok, problems, report }) {
  const fmt = (n) => n.toLocaleString('es-PE');
  return [
    '## Presupuesto del WASM de simulación',
    '',
    '| Archivo | Crudo (bytes) | gzip -9 (bytes) | Tope inclusivo |',
    '| --- | --- | --- | --- |',
    ...report.map((r) => `| \`${r.path}\` | ${fmt(r.raw)} | ${fmt(r.gzip)} | ${fmt(r.limit)} |`),
    '',
    ok ? '**Dentro del presupuesto.**' : problems.map((p) => `- ❌ ${p}`).join('\n'),
  ].join('\n');
}

if (process.argv[1] && resolve(process.argv[1]) === fileURLToPath(import.meta.url)) {
  const arg = (k, d) => {
    const i = process.argv.indexOf(k);
    return i > 0 ? process.argv[i + 1] : d;
  };
  const result = checkWasm(resolve(arg('--dist', join(repo, 'apps/web/dist'))), Number(arg('--limit', LIMIT)));
  console.log(markdown(result));
  process.exit(result.ok ? 0 : 1);
}
