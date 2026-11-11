import test from 'node:test';
import assert from 'node:assert/strict';
import { mkdirSync, mkdtempSync, rmSync, writeFileSync } from 'node:fs';
import { tmpdir } from 'node:os';
import { join } from 'node:path';
import { checkBuilds, readMarker } from './check-devtools.mjs';

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

test('reads the marker from the devtools source', () => {
  assert.equal(readMarker(`export const DEVTOOLS_MARKER = '${MARKER}';`), MARKER);
  assert.throws(() => readMarker('nothing here'));
});
