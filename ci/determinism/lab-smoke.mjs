import { readFile } from 'node:fs/promises';
import assert from 'node:assert/strict';
import { chromium } from 'playwright';
import { createRequire } from 'node:module';
import { fileURLToPath, pathToFileURL } from 'node:url';

const root = fileURLToPath(new URL('../../apps/web/', import.meta.url));
const requireWeb = createRequire(new URL('../../apps/web/package.json', import.meta.url));
const { preview, createServer } = await import(pathToFileURL(requireWeb.resolve('vite')).href);
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

/** AR28 on test-loop: record a golden, jump to the first mismatch of an altered replay, teleport. */
async function devtoolsAr28(page) {
  await page.getByRole('tab', { name: 'Depuración' }).click();
  const golden = page.waitForEvent('download');
  await page.getByRole('button', { name: 'Grabar dorado' }).click();
  const file = await golden;
  assert.equal(file.suggestedFilename(), 'test-loop.lab.json');
  assert.equal(JSON.parse(await readFile(await file.path(), 'utf8')).format, 'race-lab-replay/1');

  // A golden with one input run changed on purpose diverges; the lab re-simulates up to that tick.
  const source = new URL('../../crates/sim-core/tests/golden/test-loop.scripted.json', import.meta.url);
  const replay = JSON.parse(await readFile(source, 'utf8'));
  const k = Math.floor(replay.inputs.length / 2);
  replay.inputs[k][1] = (replay.inputs[k][1] ^ 0x7f) >>> 0;
  await page.getByRole('tab', { name: 'Sesión' }).click();
  await page
    .locator('input[type=file]')
    .setInputFiles({ name: 'altered.json', mimeType: 'application/json', buffer: Buffer.from(JSON.stringify(replay)) });
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

const server = dev ? await createServer(config('development')) : await preview(config('lab'));
if (dev) await server.listen();
const publication = dev ? null : await preview(config('production'));
let browser;
try {
  browser = await chromium.launch();
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
      .setInputFiles({ name: 'invalid.json', mimeType: 'application/json', buffer: Buffer.from('{') });
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
        return JSON.parse(await readFile(await (await file).path(), 'utf8')).ticks;
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
