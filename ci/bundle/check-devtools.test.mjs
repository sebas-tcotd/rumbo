import test from 'node:test';
import assert from 'node:assert/strict';
import { mkdirSync, mkdtempSync, rmSync, writeFileSync } from 'node:fs';
import { tmpdir } from 'node:os';
import { join } from 'node:path';
import { checkBuilds, devtoolsFingerprints, readMarker } from './check-devtools.mjs';

const MARKER = 'rumbo-devtools-bundle-marker';

function builds(t, { dist, lab }) {
  const root = mkdtempSync(join(tmpdir(), 'bundle-check-'));
  t.after(() => rmSync(root, { recursive: true, force: true }));
  const write = (dir, files) => {
    for (const [name, text] of Object.entries(files)) {
      mkdirSync(join(root, dir, 'assets'), { recursive: true });
      writeFileSync(join(root, dir, 'assets', name), text);
    }
  };
  write('dist', dist);
  write('dist-lab', lab);
  return { dist: join(root, 'dist'), distLab: join(root, 'dist-lab'), marker: MARKER };
}

test('passes when only the lab build has devtools', (t) => {
  const dirs = builds(t, { dist: { 'index.js': 'app' }, lab: { 'index.js': 'app', 'devtools-x.js': MARKER } });
  assert.deepEqual(checkBuilds(dirs), []);
});

test('fails when the marker leaks into the publication build', (t) => {
  const dirs = builds(t, { dist: { 'index.js': `x="${MARKER}"` }, lab: { 'devtools-x.js': MARKER } });
  assert.match(checkBuilds(dirs).join('\n'), /publication build contains devtools/);
});

test('fails on a devtools chunk in dist even without the marker', (t) => {
  const dirs = builds(t, { dist: { 'index.js': 'app', 'devtools-a.css': '' }, lab: { 'devtools-x.js': MARKER } });
  assert.match(checkBuilds(dirs).join('\n'), /emits a devtools chunk/);
});

test('fails when the lab build lacks devtools', (t) => {
  const dirs = builds(t, { dist: { 'index.js': 'app' }, lab: { 'index.js': 'app' } });
  assert.match(checkBuilds(dirs).join('\n'), /lab build is missing devtools/);
});

const FINGERPRINTS = { texts: ['Laboratorio de manejo'], classes: ['panel-head'] };

test('fails on a partial leak: a devtools text without the marker', (t) => {
  const dirs = builds(t, { dist: { 'index.js': 'h("h1",null,"Laboratorio de manejo")' }, lab: { 'a.js': MARKER } });
  assert.match(checkBuilds({ ...dirs, fingerprints: FINGERPRINTS }).join('\n'), /devtools text "Laboratorio de manejo"/);
});

test('fails on devtools styles in the publication CSS, not on a class name prefix', (t) => {
  const leak = builds(t, { dist: { 'index.js': 'app', 'index.css': '.panel-head{color:red}' }, lab: { 'a.js': MARKER } });
  assert.match(checkBuilds({ ...leak, fingerprints: FINGERPRINTS }).join('\n'), /devtools styles \(\.panel-head\)/);
  const ok = builds(t, { dist: { 'index.js': 'app', 'index.css': '.panel-header{color:red}' }, lab: { 'a.js': MARKER } });
  assert.deepEqual(checkBuilds({ ...ok, fingerprints: FINGERPRINTS }), []);
});

test('reports a missing build instead of throwing', () => {
  assert.match(checkBuilds({ dist: 'no/such/dist', distLab: 'no/such/lab', marker: MARKER }).join('\n'), /missing/);
});

test('fingerprints keep devtools prose and classes the shipped code does not have', () => {
  const fp = devtoolsFingerprints({
    devtoolsSources: ['<h1>Laboratorio de manejo</h1><p>Piloto automático</p>{useState<string | null>(null)}<b>x</b>'],
    devtoolsCss: ['.panel-head{} .hud{}'],
    shippedSources: ['"autopilot": "Piloto automático"'],
    shippedCss: ['.hud{}'],
  });
  assert.deepEqual(fp, { texts: ['Laboratorio de manejo'], classes: ['panel-head'] });
});

test('reads the marker from the devtools source', () => {
  assert.equal(readMarker(`export const DEVTOOLS_MARKER = '${MARKER}';`), MARKER);
  assert.throws(() => readMarker('nothing here'));
});
