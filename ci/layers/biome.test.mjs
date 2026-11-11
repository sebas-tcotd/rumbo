import assert from 'node:assert/strict';
import { mkdtempSync, rmSync, writeFileSync } from 'node:fs';
import { tmpdir } from 'node:os';
import { join } from 'node:path';
import test from 'node:test';
import { biomeText, climb, isInSync, overrides } from './biome.mjs';
import { NESTED_DEPTHS, NO_UNNORMALIZED, NO_WASM, TS } from './policy.mjs';

const groups = (entry) => entry.linter.rules.style.noRestrictedImports.options.patterns.map((p) => p.group.join());

test('biome.json on disk is the generated file', () => {
  assert.equal(isInSync(), true, 'run `node ci/layers/biome.mjs`');
});

test('a drifted biome.json is detected, CRLF is not drift', () => {
  const dir = mkdtempSync(join(tmpdir(), 'biome-'));
  try {
    const path = join(dir, 'biome.json');
    writeFileSync(path, biomeText().replace(/\n/g, '\r\n'));
    assert.equal(isInSync(path), true);
    writeFileSync(path, biomeText().replace('"error"', '"warn"'));
    assert.equal(isInSync(path), false);
  } finally {
    rmSync(dir, { recursive: true, force: true });
  }
});

test('every override carries the package prohibitions, the WASM rule and the normalization rule', () => {
  const list = overrides();
  for (const entry of list) {
    const dir = Object.values(TS).find((layer) => entry.includes[0].startsWith(layer.dir + '/'));
    const expected = dir.forbid.map((rule) => rule.group.join());
    assert.deepEqual(groups(entry).slice(0, expected.length), expected, entry.includes[0]);
    assert.ok(groups(entry).includes(NO_WASM.group.join()), entry.includes[0]);
    assert.ok(groups(entry).includes(NO_UNNORMALIZED.group.join()), entry.includes[0]);
  }
});

test('nested packages get one wildcard plus one override per depth, escapes climbing d + 1', () => {
  const ui = overrides().filter((entry) => entry.includes[0].startsWith('packages/ui/'));
  assert.equal(ui.length, NESTED_DEPTHS + 1);
  assert.ok(groups(ui[0]).includes(climb(NESTED_DEPTHS + 1)));
  for (let depth = 0; depth < NESTED_DEPTHS; depth++) assert.ok(groups(ui[depth + 1]).includes(climb(depth + 1)));
});

test('isolated subfolders come after the package overrides and block climbing out of the subfolder', () => {
  const list = overrides().map((entry) => entry.includes[0]);
  const lastPackage = list.lastIndexOf('packages/platform-web/*/*/*.{ts,tsx,mjs}');
  const input = list.indexOf('packages/platform-web/input/*.{ts,tsx,mjs}');
  assert.ok(input > lastPackage, 'subfolder overrides must win over the package ones');
  const entry = overrides().find((item) => item.includes[0] === 'packages/platform-web/input/*.{ts,tsx,mjs}');
  assert.ok(groups(entry).includes(climb(1)));
});

test('tooling and generated packages get no overrides', () => {
  const dirs = Object.values(TS).filter((layer) => layer.tooling || layer.generated).map((layer) => layer.dir);
  for (const entry of overrides()) assert.ok(!dirs.some((dir) => entry.includes[0].startsWith(dir + '/')));
});
