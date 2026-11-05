// S4 · Re-simula los replays dorados (grabados en nativo) con el binario WASM y exige los
// mismos hashes de control en cada entorno.
//
// Uso: node run.mjs --browser node|chromium|firefox|webkit|safari [--out dir]
// Requiere el WASM compilado en crates/sim-wasm/pkg (pnpm turbo run build --filter @rumbo/sim-wasm).
import { readFileSync, writeFileSync, mkdirSync } from 'node:fs';
import { dirname, join, resolve } from 'node:path';
import { fileURLToPath, pathToFileURL } from 'node:url';
import { listCases } from './cases.mjs';
import { serve } from './server.mjs';

const here = dirname(fileURLToPath(import.meta.url));
const root = resolve(here, '../..');
const arg = (k, d) => {
  const i = process.argv.indexOf(k);
  return i > 0 ? process.argv[i + 1] : d;
};
const browser = arg('--browser', 'node');
const outDir = resolve(arg('--out', join(here, 'results')));
const cases = listCases(root);
const TIMEOUT_MS = 20 * 60 * 1000;

async function runNode() {
  const pkg = join(root, 'crates/sim-wasm/pkg');
  const { default: init, verify_replay, physics_version } = await import(pathToFileURL(join(pkg, 'sim_wasm.js')).href);
  await init({ module_or_path: readFileSync(join(pkg, 'sim_wasm_bg.wasm')) });
  const results = cases.map((c) => {
    const t0 = performance.now();
    const rep = JSON.parse(verify_replay(readFileSync(join(root, c.track), 'utf8'), readFileSync(join(root, c.replay), 'utf8')));
    return {
      case: c.name, ok: rep.ok, message: rep.message, ticks: rep.ticks, expected: c.expected,
      actual: rep.actualFinal, firstMismatch: rep.firstMismatchTick ?? null, ms: Math.round(performance.now() - t0),
    };
  });
  return { userAgent: `Node ${process.version}`, physics: physics_version(), results };
}

async function runPlaywright(name, url) {
  const pw = await import('playwright');
  const b = await pw[name].launch();
  try {
    const page = await b.newPage();
    page.on('pageerror', (e) => console.error('error en la página:', e.message));
    await page.goto(url);
    await page.waitForFunction(() => window.__determinism, null, { timeout: TIMEOUT_MS, polling: 1000 });
    return await page.evaluate(() => window.__determinism);
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

mkdirSync(outDir, { recursive: true });
writeFileSync(join(outDir, `results-${browser}.json`), JSON.stringify({ browser, ...out }, null, 2));
console.log(`${browser} · ${out.userAgent} · física ${out.physics ?? '?'}`);
if (out.error) console.error(out.error);
let failed = out.error ? 1 : 0;
for (const r of out.results) {
  console.log(`${r.ok ? 'OK   ' : 'FALLA'} ${r.case.padEnd(36)} ${String(r.actual ?? '').padEnd(16)} ${r.ms} ms${r.ok ? '' : ` · ${r.message} (primer desvío en el tick ${r.firstMismatch})`}`);
  if (!r.ok) failed++;
}
if (out.results.length !== cases.length) {
  console.error(`solo ${out.results.length} de ${cases.length} casos`);
  failed++;
}
console.log(failed ? `${failed} problemas` : `los ${cases.length} replays dan los mismos hashes que en nativo`);
process.exit(failed ? 1 : 0);
