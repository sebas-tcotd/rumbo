import assert from 'node:assert/strict';
import test from 'node:test';
import { spawnSync } from 'node:child_process';
import { mkdirSync, mkdtempSync, writeFileSync, rmSync } from 'node:fs';
import { fileURLToPath } from 'node:url';
import { dirname, join } from 'node:path';
const root = fileURLToPath(new URL('../../', import.meta.url));
const biome = join(root, 'node_modules/@biomejs/biome/bin/biome');

const ESCAPE = 'export * from "../../contracts/protocol";';
const WASM = 'import wasm from "./sim.wasm";';

/** Per package: what must pass (positive control) and what must be rejected (type, subpath, dynamic, escape, wasm). */
const LAYERS = {
  contracts: {
    allowed: 'import type { SimPort } from "./ports.js"; export type { SimPort };',
    rejected: [
      'import type { X } from "react";',
      'import "three/examples/test";',
      'const load = () => import("@rumbo/game");',
      'import "react-dom/client";',
      'import "@rumbo/sim-wasm";',
    ],
  },
  'sim-worker': {
    allowed:
      'import type { SimPort } from "@rumbo/contracts"; import init from "@rumbo/sim-wasm"; export { init }; export type { SimPort };',
    rejected: [
      'import type { X } from "react";',
      'import "three/examples/test";',
      'const load = () => import("@rumbo/game");',
      'import "react-dom/client";',
    ],
  },
  'render-three': {
    allowed:
      'import type { RendererPort } from "@rumbo/contracts"; import * as THREE from "three/webgpu"; import { SCENE } from "../palette.ts"; export { THREE, SCENE }; export type { RendererPort };',
    rejected: [
      'import type { FC } from "react";',
      'import "react-dom/client";',
      'const load = () => import("@rumbo/ui");',
      'import "@rumbo/platform-web";',
      'import type { SimPort } from "@rumbo/sim-worker";',
    ],
  },
  'platform-web': {
    allowed:
      'import type { SettingsStorePort } from "@rumbo/contracts"; import { x } from "../storage/index.ts"; export { x }; export type { SettingsStorePort };',
    rejected: [
      'import type { FC } from "react";',
      'import "three/webgpu";',
      'const load = () => import("@rumbo/render-three");',
      'import "@rumbo/sim-worker";',
      'import "@rumbo/ui";',
    ],
  },
  ui: {
    allowed:
      'import type { Settings } from "@rumbo/contracts"; import { createRoot } from "react-dom/client"; import { useState } from "react"; import i18next from "i18next"; import { I18nextProvider } from "react-i18next"; export { createRoot, useState, i18next, I18nextProvider }; export type { Settings };',
    rejected: [
      'import type { Scene } from "three";',
      'import "three/webgpu";',
      'const load = () => import("@rumbo/render-three");',
      'import "@rumbo/platform-web";',
      'import "@rumbo/sim-worker";',
      'import "@rumbo/sim-wasm";',
      'import "@rumbo/devtools";',
    ],
  },
  devtools: {
    allowed:
      'import type { RenderDebugPort } from "@rumbo/contracts"; import { createRoot } from "react-dom/client"; import { useRef } from "react"; export { createRoot, useRef }; export type { RenderDebugPort };',
    rejected: [
      'import type { Scene } from "three";',
      'const load = () => import("@rumbo/ui");',
      'import "@rumbo/render-three";',
      'import "@rumbo/platform-web";',
      'import type { SimPort } from "@rumbo/sim-worker";',
      'import "@rumbo/sim-wasm";',
    ],
  },
};

for (const [name, { allowed, rejected }] of Object.entries(LAYERS)) {
  test(name + ' rejects cross-layer imports', () => {
    // A subfolder one level deep: a `../` import back into the package must stay legal there.
    const dir = mkdtempSync(join(root, 'packages', name, 'layer-test-'));
    try {
      const lint = (source) => {
        const path = join(dir, 'case.ts');
        writeFileSync(path, source);
        return spawnSync(process.execPath, [biome, 'lint', path], { cwd: root, encoding: 'utf8' });
      };
      // Positive control: what the layer is allowed to import must pass.
      const ok = lint(allowed);
      assert.equal(ok.status, 0, 'allowed imports must pass: ' + allowed + '\n' + ok.stderr);
      for (const source of [...rejected, ESCAPE, WASM]) {
        const result = lint(source);
        assert.equal(result.status, 1, source);
        assert.match(result.stderr, /noRestrictedImports/, source);
      }
    } finally {
      rmSync(dir, { recursive: true, force: true });
    }
  });
}

/** The four front packages also lint .tsx and .mjs files, at any folder depth. */
const DEEP = ['render-three', 'platform-web', 'ui', 'devtools'];
const deepCases = (name) => [
  ['x/case.tsx', LAYERS[name].rejected[0]],
  ['x/y/case.mjs', LAYERS[name].rejected[1]],
  ['x/y/z/case.tsx', LAYERS[name].rejected[0]],
  ['x/y/z/w/case.ts', LAYERS[name].rejected[1]],
  ['x/y/z/case.ts', 'export * from "../../../../contracts/protocol";'],
];

for (const name of DEEP) {
  test(name + ' keeps its layer rules in deep folders, .tsx and .mjs', () => {
    const dir = mkdtempSync(join(root, 'packages', name, 'layer-test-'));
    const lint = (rel, source) => {
      const path = join(dir, rel);
      mkdirSync(dirname(path), { recursive: true });
      writeFileSync(path, source);
      return spawnSync(process.execPath, [biome, 'lint', path], { cwd: root, encoding: 'utf8' });
    };
    try {
      const ok = lint('x/y/z/allowed.tsx', LAYERS[name].allowed);
      assert.equal(ok.status, 0, 'allowed imports must pass three folders deep\n' + ok.stderr);
      for (const [rel, source] of deepCases(name)) {
        const result = lint(rel, source);
        assert.equal(result.status, 1, rel + ': ' + source);
        assert.match(result.stderr, /noRestrictedImports/, rel);
      }
    } finally {
      rmSync(dir, { recursive: true, force: true });
    }
  });
}

test('devtools rejects three subpaths', () => {
  const dir = mkdtempSync(join(root, 'packages', 'devtools', 'layer-test-'));
  try {
    const path = join(dir, 'case.tsx');
    writeFileSync(path, 'import "three/webgpu";');
    const result = spawnSync(process.execPath, [biome, 'lint', path], { cwd: root, encoding: 'utf8' });
    assert.equal(result.status, 1);
    assert.match(result.stderr, /noRestrictedImports/);
  } finally {
    rmSync(dir, { recursive: true, force: true });
  }
});

test('a package root file cannot climb out with a single ../', () => {
  const path = join(root, 'packages', 'ui', 'layer-root-case.ts');
  try {
    writeFileSync(path, 'import "../contracts/index.ts";');
    const result = spawnSync(process.execPath, [biome, 'lint', path], { cwd: root, encoding: 'utf8' });
    assert.equal(result.status, 1);
    assert.match(result.stderr, /noRestrictedImports/);
  } finally {
    rmSync(path, { force: true });
  }
});
