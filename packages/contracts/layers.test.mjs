import assert from 'node:assert/strict';
import test from 'node:test';
import { spawnSync } from 'node:child_process';
import { mkdtempSync, writeFileSync, rmSync } from 'node:fs';
import { fileURLToPath } from 'node:url';
import { join } from 'node:path';
const root = fileURLToPath(new URL('../../', import.meta.url));
const biome = join(root, 'node_modules/@biomejs/biome/bin/biome');
for (const name of ['contracts', 'sim-worker']) {
  test(name + ' rejects cross-layer imports', () => {
    const dir = mkdtempSync(join(root, 'packages', name, 'layer-test-'));
    try {
      const cases = [
        'import type { X } from "react";',
        'import "three/examples/test";',
        'const load = () => import("@rumbo/game");',
        'export * from "../../contracts/protocol";',
        'import "react-dom/client";',
        'import wasm from "./sim.wasm";',
      ];
      if (name === 'contracts') cases.push('import "@rumbo/sim-wasm";');
      const lint = (source) => {
        const path = join(dir, 'case.ts');
        writeFileSync(path, source);
        return spawnSync(process.execPath, [biome, 'lint', path], { cwd: root, encoding: 'utf8' });
      };
      // Positive control: what the layer is allowed to import must pass.
      const allowed =
        name === 'contracts'
          ? 'import type { SimPort } from "./ports.js"; export type { SimPort };'
          : 'import type { SimPort } from "@rumbo/contracts"; import init from "@rumbo/sim-wasm"; export { init }; export type { SimPort };';
      assert.equal(lint(allowed).status, 0, 'allowed imports must pass: ' + allowed);
      for (const source of cases) {
        const path = join(dir, 'negative.ts');
        writeFileSync(path, source);
        const result = spawnSync(process.execPath, [biome, 'lint', path], { cwd: root, encoding: 'utf8' });
        assert.equal(result.status, 1, source);
        assert.match(result.stderr, /noRestrictedImports/, source);
      }
    } finally {
      rmSync(dir, { recursive: true, force: true });
    }
  });
}
