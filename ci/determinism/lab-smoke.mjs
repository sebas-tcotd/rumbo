import { readFile } from 'node:fs/promises';
import assert from 'node:assert/strict';
import { chromium, firefox, webkit } from 'playwright';
import { createRequire } from 'node:module';
import { fileURLToPath, pathToFileURL } from 'node:url';
import { join } from 'node:path';

const root = fileURLToPath(new URL('../../apps/web/', import.meta.url));
const requireWeb = createRequire(new URL('../../apps/web/package.json', import.meta.url));
const { preview, createServer } = await import(pathToFileURL(requireWeb.resolve('vite')).href);
// Exported replays are `rumbo-replay/2` bytes: their metadata is read through the Rust codec (WASM), never in JS.
const { default: initSim, replay_summary } = await import(
  pathToFileURL(fileURLToPath(new URL('../../crates/sim-wasm/pkg/sim_wasm.js', import.meta.url))).href
);
await initSim({ module_or_path: await readFile(new URL('../../crates/sim-wasm/pkg/sim_wasm_bg.wasm', import.meta.url)) });
const summaryOf = async (download) => JSON.parse(replay_summary(await readFile(await download.path())));
const dev = process.argv.includes('--dev');
const physicsVersion = /PHYSICS_VERSION: &str = "([^"]+)"/.exec(
  await readFile(new URL('../../crates/track-format/src/version.rs', import.meta.url), 'utf8'),
)?.[1];
assert.ok(physicsVersion, 'physics version not found');
// The lab smoke drives the devtools panel, so it previews the lab build (dist-lab/, mode 'lab').
// The publication build (dist/) gets its own minimal check below: it loads, drives, shows the HUD and
// has no panel. Both builds come from `pnpm turbo run build`.
const config = (mode) => ({
  root,
  mode,
  configFile: root + '/vite.config.ts',
  server: { port: 0, host: '127.0.0.1' },
  preview: { port: 0, host: '127.0.0.1' },
});
const telemetry = (page, label) =>
  page
    .locator('dt')
    .filter({ hasText: new RegExp('^' + label + '$') })
    .locator('..')
    .locator('dd')
    .innerText();
/** Waits until the telemetry value next to `label` satisfies `accept` (both run in the page). */
const waitTelemetry = (page, label, accept, arg) =>
  page.waitForFunction(
    ([label, accept, arg]) => {
      const dd = [...document.querySelectorAll('dt')].find((dt) => dt.textContent === label)?.nextElementSibling;
      return dd ? new Function('value', 'arg', 'return ' + accept)(dd.textContent, arg) : false;
    },
    [label, accept, arg],
  );

/**
 * AR28 on test-loop: record a golden, jump to the first mismatch of a replay, teleport. The lab runs a tuned
 * profile at this point (the caller applied one), and a `rumbo-replay/2` does not carry its profile (D17):
 * verifying the default-profile golden with the lab's profile diverges, as an altered replay would.
 */
async function devtoolsAr28(page) {
  await page.getByRole('tab', { name: 'Depuración' }).click();
  const golden = page.waitForEvent('download');
  await page.getByRole('button', { name: 'Grabar dorado' }).click();
  const file = await golden;
  assert.equal(file.suggestedFilename(), 'test-loop.lab.bin');
  assert.equal((await summaryOf(file)).header.format, 'rumbo-replay/2');

  // The golden diverges under the lab's tuned profile; the lab re-simulates up to the first mismatch.
  const source = new URL('../../crates/sim-core/tests/golden/test-loop.scripted.bin', import.meta.url);
  const bytes = await readFile(source);
  const replay = JSON.parse(replay_summary(bytes));
  await page.getByRole('tab', { name: 'Sesión' }).click();
  await page
    .locator('input[type=file]')
    .setInputFiles({ name: 'test-loop.scripted.bin', mimeType: 'application/octet-stream', buffer: bytes });
  const seek = page.getByRole('button', { name: /^Ir al tick \d+$/ });
  await seek.waitFor();
  const tick = Number((await seek.innerText()).match(/\d+/)[0]);
  assert.ok(tick > 0 && tick <= replay.ticks, String(tick));
  await seek.click();
  await page.getByText('Sesión alterada', { exact: false }).waitFor();
  await page.locator('.badge', { hasText: 'En pausa' }).waitFor();
  await page.getByRole('tab', { name: 'Telemetría' }).click();
  await waitTelemetry(page, 'Tick', 'value === String(arg)', tick);

  // Teleport: the car lands at s while paused, and an altered session cannot be recorded as a golden.
  await page.getByRole('tab', { name: 'Depuración' }).click();
  assert.equal(await page.getByRole('button', { name: 'Grabar dorado' }).isDisabled(), true);
  await page.getByLabel(/Distancia s/).fill('200');
  await page.getByRole('button', { name: 'Teletransportar' }).click();
  await page.getByRole('tab', { name: 'Telemetría' }).click();
  await waitTelemetry(page, 'Progreso', 'Math.abs(Number(value.split("/")[0]) - arg) < 5', 200);
  assert.match(await telemetry(page, 'Progreso'), /^\d+ \/ \d+ m$/);
  await page.keyboard.press('Escape');
}

const fatal = (page) => page.getByRole('alertdialog', { name: 'Algo falló' });
const retry = (page) => page.getByRole('button', { name: 'Reintentar' });
const copy = (page) => page.getByRole('button', { name: 'Copiar diagnóstico' }).click();

async function inFreshPage(browser, run) {
  const context = await browser.newContext();
  try {
    await run(await context.newPage(), context);
  } finally {
    await context.close();
  }
}

async function screenshot(page, file) {
  if (process.env.RUMBO_SMOKE_SCREENSHOTS) {
    await page.screenshot({ path: join(process.env.RUMBO_SMOKE_SCREENSHOTS, file) });
  }
}

async function checkWasmFailure(page, context, url) {
  await page.route('**/*.wasm', (route) => route.abort());
  await context.grantPermissions(['clipboard-read', 'clipboard-write']);
  await page.goto(url);
  await fatal(page).getByText('No se pudo cargar la simulación.').waitFor();
  assert.equal(await retry(page).evaluate((button) => button === document.activeElement), true);
  await screenshot(page, '0-8-wasm-fatal.png');
  await copy(page);
  await page.getByRole('status').getByText('Diagnóstico copiado.').waitFor();
  const diagnostic = await page.evaluate(() => navigator.clipboard.readText());
  assert.match(diagnostic, /error: WASM_LOAD_FAILED \(fatal, channel sim\)/);
  assert.match(diagnostic, /build: [\da-f]{7,}|build: dev/);
  assert.match(diagnostic, /detail: Failed to fetch/);
  await Promise.all([page.waitForEvent('framenavigated'), retry(page).click()]);
  await fatal(page).getByText('No se pudo cargar la simulación.').waitFor();
}

async function checkMissingApi(page, url, capability) {
  const workers = [];
  const wasm = [];
  page.on('worker', (worker) => workers.push(worker.url()));
  page.on('request', (request) => { if (request.url().endsWith('.wasm')) wasm.push(request.url()); });
  await page.addInitScript((missing) => {
    if (missing === 'compressionStream') Object.defineProperty(window, 'CompressionStream', { value: undefined });
    if (missing === 'webgl2') {
      const getContext = HTMLCanvasElement.prototype.getContext;
      HTMLCanvasElement.prototype.getContext = function (name, ...args) {
        return name === 'webgl2' ? null : getContext.call(this, name, ...args);
      };
    }
  }, capability);
  await page.goto(url);
  const label = capability === 'webgl2' ? 'WebGL 2' : 'Compresión y descompresión (deflate-raw)';
  await fatal(page).getByText(label, { exact: true }).waitFor();
  await fatal(page).getByRole('link', { name: 'Ver navegadores soportados' }).waitFor();
  assert.equal(await page.locator('.render-host canvas').count(), 0);
  assert.equal(await fatal(page).evaluate((dialog) => dialog.clientHeight), await page.evaluate(() => innerHeight));
  assert.deepEqual(workers, []);
  assert.deepEqual(wasm, []);
  if (capability === 'compressionStream') await screenshot(page, '0-8-browser-unsupported.png');
}

async function checkGamepadFallback(page, url, transient) {
  await page.addInitScript((transient) => {
    let calls = 0;
    Object.defineProperty(navigator, 'getGamepads', { value: transient ? () => {
      if (++calls === 1) throw new Error('blocked during capability check');
      return [];
    } : undefined });
    const requestFrame = window.requestAnimationFrame.bind(window);
    window.testFrameRequests = 0;
    window.requestAnimationFrame = (callback) => {
      window.testFrameRequests++;
      return requestFrame(callback);
    };
    Object.defineProperty(navigator, 'clipboard', { value: { writeText: () => Promise.reject(new Error('denied')) } });
    const BrowserWorker = window.Worker;
    window.Worker = class extends BrowserWorker {
      constructor(...args) {
        super(...args);
        window.testSimWorker = this;
      }
    };
  }, transient);
  await page.goto(url);
  await page.locator('.hud .speed-unit').waitFor();
  await page.keyboard.down('w');
  await page.waitForFunction(() => Number(document.querySelector('.speed-value')?.textContent) > 5);
  await page.keyboard.up('w');
  // Keep the successful driving path and exercise diagnostic export through a simulated worker fault.
  await page.evaluate(() => {
    const data = { type: 'fatal', code: 'SIM_FAILED', message: String.raw`open \\example.invalid\share\Alice Doe\replay.json at [::]`,
      generation: 1, protocolVersion: 3 };
    window.testSimWorker.dispatchEvent(new MessageEvent('message', { data }));
  });
  await fatal(page).getByText('La simulación se detuvo.', { exact: false }).waitFor();
  assert.equal(await page.locator('.hud').count(), 0);
  await copy(page);
  const manual = page.getByRole('textbox', { name: 'Diagnóstico para copiar a mano' });
  await manual.waitFor();
  const diagnostic = await manual.inputValue();
  assert.match(diagnostic, new RegExp(`input GAMEPAD_UNAVAILABLE reason=${transient ? 'blocked' : 'missing'}`));
  assert.equal((diagnostic.match(/GAMEPAD_UNAVAILABLE/g) ?? []).length, 1);
  assert.match(diagnostic, /gamepad=no/);
  assert.doesNotMatch(diagnostic, /example\.invalid|Alice Doe|\[::\]/);
  await copy(page);
  await page.waitForFunction(() => {
    const area = document.querySelector('.fatal textarea');
    return area?.readOnly && area === document.activeElement && area.selectionEnd === area.value.length && area.selectionStart === 0;
  });
  const frameRequests = await page.evaluate(async () => {
    const before = window.testFrameRequests;
    await new Promise((resolve) => setTimeout(resolve, 150));
    return window.testFrameRequests - before;
  });
  assert.equal(frameRequests, 0, 'the host and renderer stop requesting frames in Fatal');
}

async function checkRendererFailure(page, url) {
  await page.addInitScript(() => {
    Object.defineProperty(navigator, 'gpu', { value: undefined });
    const getContext = HTMLCanvasElement.prototype.getContext;
    let webgl2Requests = 0;
    HTMLCanvasElement.prototype.getContext = function (name, ...args) {
      if (name === 'webgl2' && ++webgl2Requests > 1) return null;
      return getContext.call(this, name, ...args);
    };
  });
  await page.goto(url);
  await fatal(page).getByText('No se pudo iniciar la vista 3D.').waitFor();
  assert.equal(await page.locator('.render-host canvas').count(), 0);
}

async function checkModuleWorkerProbe(engine, url) {
  const browser = await engine.launch();
  try {
    await inFreshPage(browser, async (page) => {
      const workers = [];
      page.on('worker', (worker) => workers.push(worker.url()));
      await page.addInitScript(() => {
        Object.defineProperty(window, 'CompressionStream', { value: undefined });
        Object.defineProperty(navigator, 'clipboard', { value: undefined });
      });
      await page.goto(url);
      await fatal(page).getByText('Compresión y descompresión (deflate-raw)').waitFor();
      await copy(page);
      const diagnostic = await page.getByRole('textbox', { name: 'Diagnóstico para copiar a mano' }).inputValue();
      assert.match(diagnostic, /moduleWorker=yes/);
      assert.deepEqual(workers, []);
    });
    console.log('OK module worker capability probe:', engine.name());
  } finally {
    await browser.close();
  }
}

async function fatalSmoke(browser, url) {
  await inFreshPage(browser, (page, context) => checkWasmFailure(page, context, url));
  for (const api of ['compressionStream', 'webgl2']) {
    await inFreshPage(browser, (page) => checkMissingApi(page, url, api));
  }
  await inFreshPage(browser, (page) => checkRendererFailure(page, url));
  for (const transient of [false, true]) {
    await inFreshPage(browser, (page) => checkGamepadFallback(page, url, transient));
  }
  console.log('OK Fatal: WASM, browser capabilities, keyboard fallback and diagnostics');
}

const server = dev ? await createServer(config('development')) : await preview(config('lab'));
if (dev) await server.listen();
const publication = dev ? null : await preview(config('production'));
let browser;
try {
  browser = await chromium.launch();
  await fatalSmoke(browser, (publication ?? server).resolvedUrls.local[0] + '?pista=test-loop');
  if (publication) {
    const url = publication.resolvedUrls.local[0] + '?pista=test-loop';
    const engines = process.argv.includes('--capability-matrix') ? [chromium, firefox, webkit] : [chromium];
    for (const engine of engines) await checkModuleWorkerProbe(engine, url);
  }
  const page = await browser.newPage({ acceptDownloads: true });
  const errors = [];
  page.on('pageerror', (error) => errors.push(error.message));
  page.on('console', (msg) => msg.type() === 'error' && errors.push(msg.text()));
  // Window error events Playwright's pageerror misses (e.g. "ResizeObserver loop completed…").
  await page.addInitScript(() => {
    window.__windowErrors = [];
    window.addEventListener('error', (e) => window.__windowErrors.push(e.message));
  });
  const windowErrors = async () => errors.push(...(await page.evaluate(() => window.__windowErrors)));
  if (publication) {
    await page.goto(publication.resolvedUrls.local[0] + '?pista=test-loop');
    await page.locator('.hud .speed-unit').waitFor();
    await page.keyboard.down('w');
    await page.waitForFunction(() => Number(document.querySelector('.speed-value')?.textContent) > 5);
    await page.keyboard.up('w');
    // Let any lazily loaded chunk arrive before asserting it never mounted.
    await page.waitForLoadState('networkidle');
    assert.equal(await page.getByRole('tab').count(), 0, 'publication build has no lab panel');
    assert.equal(await page.locator('.panel-toggle').count(), 0);
    await windowErrors();
    console.log('OK publication (dist/) test-loop');
  }
  const base = server.resolvedUrls.local[0];
  for (const track of ['costa-verde-sprint', 'san-borja-las-artes', 'test-loop']) {
    await page.goto(base + '?pista=' + track);
    await page.getByRole('tab', { name: 'Sesión' }).click();
    await page.getByText(physicsVersion, { exact: true }).waitFor();
    assert.equal(await page.locator('select').inputValue(), track);
    for (let i = 0; i < 3; i++) await page.getByRole('button', { name: /Cámara:/ }).click();
    await page.keyboard.down('w');
    await page.keyboard.press('r');
    await page.keyboard.up('w');
    await page.keyboard.press('t');
    await page.keyboard.press('Escape');
    await page.keyboard.press('Escape');
    await page.getByLabel('Piloto automático').check();
    await page
      .locator('input[type=file]')
      .setInputFiles({ name: 'invalid.bin', mimeType: 'application/octet-stream', buffer: Buffer.from('{') });
    await page.locator('.verify.bad').waitFor();
    assert.equal(await page.locator('.fatal').count(), 0);
    await page.getByRole('button', { name: 'Verificar esta sesión' }).click();
    await page.locator('.verify.ok').waitFor();
    const download = page.waitForEvent('download');
    await page.getByRole('button', { name: 'Descargar replay' }).click();
    assert.match((await download).suggestedFilename(), /^replay-\d+-ticks/);
    const hash = () =>
      page
        .locator('dt')
        .filter({ hasText: /^Perfil$/ })
        .locator('..')
        .locator('dd')
        .innerText();
    const oldProfile = await hash();
    await page.getByRole('tab', { name: 'Ajustes' }).click();
    const number = page.locator('input.num').first();
    await number.fill(await number.getAttribute('min'));
    if (await page.getByRole('button', { name: 'Sin cambios' }).count())
      await number.fill(await number.getAttribute('max'));
    await page.getByRole('button', { name: /Aplicar/ }).click();
    await page.getByRole('tab', { name: 'Sesión' }).click();
    assert.notEqual(await hash(), oldProfile);
    await page.getByRole('button', { name: 'Reiniciar sesión' }).click();
    if (track === 'test-loop') {
      const ticks = async () => {
        const file = page.waitForEvent('download');
        await page.getByRole('button', { name: 'Descargar replay' }).click();
        return (await summaryOf(await file)).ticks;
      };
      const visibility = (hidden) =>
        page.evaluate((hidden) => {
          Object.defineProperty(document, 'hidden', { configurable: true, value: hidden });
          document.dispatchEvent(new Event('visibilitychange'));
        }, hidden);
      const settle = () => page.evaluate(() => new Promise((done) => setTimeout(done, 150)));
      await visibility(true);
      const held = await ticks();
      await settle();
      assert.equal(await ticks(), held);
      await page.keyboard.press('Escape');
      await visibility(false);
      await settle();
      assert.equal(await ticks(), held);
      await page.keyboard.press('Escape');
      await settle();
      assert.ok((await ticks()) > held);
      await devtoolsAr28(page);
    }
    // AR28 overlays: switching each one on and off must not raise errors.
    await page.getByRole('tab', { name: 'Depuración' }).click();
    for (const name of ['Eje de la pista', 'Checkpoints', 'Cajas de colisión']) {
      await page.getByLabel(name).check();
      await page.evaluate(() => new Promise((done) => requestAnimationFrame(() => requestAnimationFrame(done))));
      await page.getByLabel(name).uncheck();
    }
    await windowErrors();
    console.log('OK', dev ? 'Vite dev' : 'lab build (dist-lab/)', track);
  }
  assert.deepEqual(errors, []);
} finally {
  await browser?.close();
  await server.close();
  await publication?.close();
}
