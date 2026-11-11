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
const config = {
  root,
  configFile: root + '/vite.config.ts',
  server: { port: 0, host: '127.0.0.1' },
  preview: { port: 0, host: '127.0.0.1' },
};
const server = dev ? await createServer(config) : await preview(config);
if (dev) await server.listen();
let browser;
try {
  browser = await chromium.launch();
  const page = await browser.newPage({ acceptDownloads: true });
  const errors = [];
  page.on('pageerror', (error) => errors.push(error.message));
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
    }
    console.log('OK', dev ? 'Vite dev' : 'production', track);
  }
  assert.deepEqual(errors, []);
} finally {
  await browser?.close();
  await server.close();
}
