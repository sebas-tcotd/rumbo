// Guardrail (ADR-005): intentional layer violations must make Biome fail, and what each layer is
// allowed to import must lint clean. Cases come from ci/layers/policy.mjs. Each test writes
// throwaway files inside the real package (prefix `layer-test-` / `layer-case-`) and removes them.
//
// Known Biome gaps, covered by tsc instead (checked by hand, see ci/layers/README.md): `require()`,
// `import.meta.glob(...)` and `../a/../../x` paths are not seen by noRestrictedImports; in the
// packages tsc rejects them (TS2591, TS2339, TS2307/TS6059 under rootDir).

import assert from 'node:assert/strict';
import { spawnSync } from 'node:child_process';
import { mkdirSync, mkdtempSync, readdirSync, rmSync, writeFileSync } from 'node:fs';
import { dirname, join } from 'node:path';
import test from 'node:test';
import { fileURLToPath } from 'node:url';
import { TS } from './policy.mjs';

const ROOT = fileURLToPath(new URL('../../', import.meta.url));
const BIOME = join(ROOT, 'node_modules/@biomejs/biome/bin/biome');

const ESCAPE = 'export * from "../../contracts/protocol";';
const WASM = 'import wasm from "./sim.wasm";';
const UNNORMALIZED = 'import "./.././../contracts/index.ts";';

/** Lints one file; returns { status, stderr }. */
function lintFile(path) {
  return spawnSync(process.execPath, [BIOME, 'lint', path], { cwd: ROOT, encoding: 'utf8' });
}

/** Creates a throwaway folder in `dir`, runs `body(write)` and always removes the folder. */
function withScratch(dir, body) {
  const scratch = mkdtempSync(join(ROOT, dir, 'layer-test-'));
  try {
    body((rel, source) => {
      const path = join(scratch, rel);
      mkdirSync(dirname(path), { recursive: true });
      writeFileSync(path, source);
      return lintFile(path);
    });
  } finally {
    rmSync(scratch, { recursive: true, force: true });
  }
}

/** Writes one file at an exact path inside the repo, lints it and removes it. */
function lintAt(rel, source) {
  const path = join(ROOT, rel);
  try {
    writeFileSync(path, source);
    return lintFile(path);
  } finally {
    rmSync(path, { force: true });
  }
}

function assertPasses(result, label) {
  assert.equal(result.status, 0, label + ' must pass\n' + result.stderr);
}

function assertRejected(result, label) {
  assert.equal(result.status, 1, label + ' must be rejected');
  assert.match(result.stderr, /noRestrictedImports/, label);
}

const RULED = Object.entries(TS).filter(([, layer]) => !layer.tooling && !layer.generated);

for (const [name, layer] of RULED) {
  test(name + ' rejects cross-layer imports', () => {
    // A subfolder one level deep: a `../` import back into the package must stay legal there.
    withScratch(layer.dir, (lint) => {
      assertPasses(lint('case.ts', layer.allowed), 'allowed imports: ' + layer.allowed);
      for (const source of [...layer.rejected, ESCAPE, WASM, UNNORMALIZED]) assertRejected(lint('case.ts', source), source);
    });
  });
}

const NESTED = RULED.filter(([, layer]) => layer.layout === 'nested' && layer.rejected.length);

for (const [name, layer] of NESTED) {
  test(name + ' keeps its layer rules in deep folders, .tsx and .mjs', () => {
    const [first, second] = layer.rejected;
    const cases = [
      ['x/case.tsx', first],
      ['x/y/case.mjs', second],
      ['x/y/z/case.tsx', first],
      ['x/y/z/w/case.ts', second],
      ['x/y/z/case.ts', 'export * from "../../../../contracts/protocol";'],
      ['x/y/case.ts', UNNORMALIZED],
    ];
    withScratch(layer.dir, (lint) => {
      assertPasses(lint('x/y/z/allowed.tsx', layer.allowed), 'allowed imports three folders deep');
      for (const [rel, source] of cases) assertRejected(lint(rel, source), rel + ': ' + source);
    });
  });
}

test('devtools rejects three subpaths', () => {
  withScratch('packages/devtools', (lint) => assertRejected(lint('case.tsx', 'import "three/webgpu";'), 'three/webgpu'));
});

test('a package root file cannot climb out with a single ../', () => {
  assertRejected(lintAt('packages/ui/layer-case-root.ts', 'import "../contracts/index.ts";'), 'ui root ../');
});

const PLATFORM = TS['@rumbo/platform-web'];

test('every platform-web subfolder is declared isolated', () => {
  const skip = new Set(['dist', 'node_modules', '.turbo']);
  const onDisk = readdirSync(join(ROOT, PLATFORM.dir), { withFileTypes: true })
    .filter((entry) => entry.isDirectory() && !skip.has(entry.name) && !entry.name.startsWith('layer-test-'))
    .map((entry) => entry.name)
    .sort();
  assert.deepEqual([...PLATFORM.isolatedSubfolders].sort(), onDisk);
});

for (const sub of PLATFORM.isolatedSubfolders) {
  test(`platform-web/${sub} cannot import another subfolder or the package root`, () => {
    const other = PLATFORM.isolatedSubfolders.find((name) => name !== sub);
    const base = `${PLATFORM.dir}/${sub}/layer-case-${process.pid}.ts`;
    assertPasses(lintAt(base, `import "@rumbo/contracts"; import "./index.ts";`), `${sub}: own folder`);
    assertRejected(lintAt(base, `import "../${other}/index.ts";`), `${sub} → ${other}`);
    assertRejected(lintAt(base, 'import "../web-platform.ts";'), `${sub} → package root`);
    assertRejected(lintAt(base, 'import "three";'), `${sub}: package rules still apply`);
  });
}

test('platform-web subfolders stay isolated in deeper folders', () => {
  withScratch(`${PLATFORM.dir}/input`, (lint) => {
    // The scratch folder sits at input/layer-test-*/: depth 2 inside the package.
    assertPasses(lint('case.ts', 'import "../keyboard-gamepad-input.ts";'), 'input: back into input/');
    assertRejected(lint('case.ts', 'import "../../storage/index.ts";'), 'input depth 2 → storage');
    assertRejected(lint('a/case.ts', 'import "../../../storage/index.ts";'), 'input depth 3 → storage');
    assertRejected(lint('a/b/case.ts', 'import "../../../../storage/index.ts";'), 'input depth 4 → storage');
    assertRejected(lint('a/b/case.tsx', 'import "react";'), 'input depth 4: package rules still apply');
  });
});

test('platform-web root may wire its subfolders', () => {
  const source = 'import "./input/index.ts"; import "./storage/index.ts"; import "./zone-source/index.ts";';
  assertPasses(lintAt('packages/platform-web/layer-case-root.ts', source), 'platform-web root → subfolders');
});

test('apps/web imports packages by name, never by relative path, and no raw WASM', () => {
  withScratch('apps/web', (lint) => {
    assertRejected(lint('case.ts', 'import { mountUi } from "../../packages/ui/index.ts";'), 'apps/web depth 1 → packages/');
    assertRejected(lint('x/case.tsx', 'import "../../../packages/ui/index.ts";'), 'apps/web depth 2 → packages/');
    assertRejected(lint('case.ts', 'import "@rumbo/sim-wasm/sim_wasm_bg.wasm";'), 'apps/web raw wasm');
  });
  assertRejected(lintAt('apps/web/layer-case-root.ts', 'import "../../packages/ui/index.ts";'), 'apps/web root → packages/');
});
