// S4 · Re-simula los replays dorados (grabados en nativo) con el binario WASM de prueba y exige, en cada
// tick, el mismo hash de estado que la referencia nativa. Conserva la verificación heredada (controles,
// hash final y vueltas) de `verify_replay`.
//
// Uso: node run.mjs --browser node|chromium|edge|firefox|webkit|safari [--out dir] [--reference file]
// Requiere `pnpm --filter rumbo-determinism build` (o `pnpm turbo run build`): el WASM de prueba en
// ci/determinism/wasm/ (sim-wasm con la feature `harness`) y la referencia nativa en reference/native.json.
import { existsSync, mkdirSync, readFileSync, writeFileSync } from 'node:fs';
import { dirname, join, resolve } from 'node:path';
import { fileURLToPath, pathToFileURL } from 'node:url';
import { listCases } from './cases.mjs';
import { evaluateCase, REQUIRED_ENVS } from './evaluate.mjs';
import { serve } from './server.mjs';

const here = dirname(fileURLToPath(import.meta.url));
const root = resolve(here, '../..');
const arg = (k, d) => {
  const i = process.argv.indexOf(k);
  return i > 0 ? process.argv[i + 1] : d;
};
const browser = arg('--browser', 'node');
const outDir = resolve(arg('--out', join(here, 'results')));
const referencePath = resolve(arg('--reference', join(here, 'reference/native.json')));
const wasmDir = join(here, 'wasm');
const TIMEOUT_MS = 20 * 60 * 1000;

if (!REQUIRED_ENVS.includes(browser)) {
  console.error(`entorno desconocido: ${browser} (válidos: ${REQUIRED_ENVS.join(', ')})`);
  process.exit(2);
}
for (const [what, path] of [['WASM de prueba', join(wasmDir, 'sim_wasm_bg.wasm')], ['referencia nativa', referencePath]]) {
  if (!existsSync(path)) {
    console.error(`falta ${what}: ${path}. Ejecuta \`pnpm --filter rumbo-determinism build\`.`);
    process.exit(2);
  }
}
const cases = listCases(root);
const reference = JSON.parse(readFileSync(referencePath, 'utf8'));

async function runNode() {
  const { default: init, verify_replay, trace_replay, physics_version } = await import(pathToFileURL(join(wasmDir, 'sim_wasm.js')).href);
  await init({ module_or_path: readFileSync(join(wasmDir, 'sim_wasm_bg.wasm')) });
  const results = cases.map((c) => {
    const t0 = performance.now();
    const track = readFileSync(join(root, c.track), 'utf8');
    const replay = readFileSync(join(root, c.replay), 'utf8');
    try {
      const report = JSON.parse(verify_replay(track, replay));
      const { ticks, trace } = JSON.parse(trace_replay(track, replay));
      return { case: c.name, report, ticks, trace, ms: Math.round(performance.now() - t0) };
    } catch (e) {
      return { case: c.name, error: String(e), ms: Math.round(performance.now() - t0) };
    }
  });
  return { userAgent: `Node ${process.version}`, physics: physics_version(), results };
}

async function runPlaywright(name, url) {
  const pw = await import('playwright');
  // Edge is Chromium with the `msedge` channel: the stable Microsoft Edge installed on the runner.
  const b = name === 'edge' ? await pw.chromium.launch({ channel: 'msedge' }) : await pw[name].launch();
  try {
    const page = await b.newPage();
    page.on('pageerror', (e) => console.error('error en la página:', e.message));
    await page.goto(url);
    await page.waitForFunction(() => window.__determinism, null, { timeout: TIMEOUT_MS, polling: 1000 });
    const out = await page.evaluate(() => window.__determinism);
    if (name === 'edge') out.browserVersion = b.version();
    return out;
  } finally {
    await b.close();
  }
}

async function runSafari(url) {
  const { Builder } = await import('selenium-webdriver');
  const driver = await new Builder().forBrowser('safari').build();
  try {
    await driver.get(url);
    const t0 = Date.now();
    for (;;) {
      const r = await driver.executeScript('return window.__determinism || null');
      if (r) return r;
      if (Date.now() - t0 > TIMEOUT_MS) throw new Error('Safari no terminó a tiempo');
      await new Promise((ok) => setTimeout(ok, 2000));
    }
  } finally {
    await driver.quit();
  }
}

let out;
try {
  if (browser === 'node') {
    out = await runNode();
  } else {
    const srv = await serve(root, cases, join(here, 'page.html'));
    try {
      out = browser === 'safari' ? await runSafari(srv.url) : await runPlaywright(browser, srv.url);
    } finally {
      srv.close();
    }
  }
} catch (e) {
  out = { userAgent: '?', error: String(e?.stack || e), results: [] };
}

mkdirSync(outDir, { recursive: true });
writeFileSync(join(outDir, `results-${browser}.json`), JSON.stringify({ browser, ...out }));
console.log(`${browser} · ${out.userAgent}${out.browserVersion ? ` (${out.browserVersion})` : ''} · física ${out.physics ?? '?'}`);
let failed = 0;
if (out.error) {
  console.error(out.error);
  failed++;
}
if (out.physics !== undefined && out.physics !== reference.physicsVersion) {
  console.error(`física ${out.physics}, referencia nativa ${reference.physicsVersion}`);
  failed++;
}
const refBy = new Map(reference.cases.map((c) => [c.name, c]));
const resBy = new Map(out.results.map((r) => [r.case, r]));
for (const c of cases) {
  const r = resBy.get(c.name);
  const { ok, problems, firstMismatch } = evaluateCase(c, refBy.get(c.name), r);
  const where = firstMismatch === null ? '' : ` · entorno ${browser}, replay ${c.name}, primer tick distinto ${firstMismatch}`;
  console.log(`${ok ? 'OK   ' : 'FALLA'} ${c.name.padEnd(36)} ${String(r?.report?.actualFinal ?? '').padEnd(16)} ${r?.ms ?? '?'} ms${ok ? '' : `${where} · ${problems.join('; ')}`}`);
  if (!ok) failed++;
}
console.log(failed ? `${failed} problemas` : `los ${cases.length} replays dan el mismo hash que nativo en cada tick`);
process.exit(failed ? 1 : 0);
