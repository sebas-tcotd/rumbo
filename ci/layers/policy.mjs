// Layer policy (ADR-005): the single source for the Biome overrides (biome.mjs), the meta-check
// (check.mjs) and the guardrail tests. Every pnpm workspace package and every Cargo member must
// appear here; check.mjs fails when one is missing or when a listed one no longer exists.
//
// Adding a package or crate: add its entry here, run `node ci/layers/biome.mjs` to regenerate
// biome.json, and the guardrail tests pick up its cases from `rejected`/`allowed`.

const REACT = ['react', 'react/**', 'react-dom', 'react-dom/**'];
const THREE = ['three', 'three/**'];

const NO_UI_OR_RENDER = {
  group: [...REACT, ...THREE],
  message: 'ADR-005: no UI or render libraries here; expose behavior through a port in @rumbo/contracts.',
};
const NO_REACT = {
  group: REACT,
  message:
    'ADR-005: no React here; UI lives in @rumbo/ui (player) or @rumbo/devtools (lab) and talks to this layer through ports in @rumbo/contracts.',
};
const NO_THREE = {
  group: THREE,
  message:
    'ADR-005: three is used only by @rumbo/render-three; reach the renderer through RendererPort/RenderDebugPort injected from apps/web.',
};
/** Rejected in every package, composition root included. */
export const NO_WASM = {
  group: ['**/*.wasm'],
  message: 'ADR-005: WASM is loaded only inside the adapter through @rumbo/sim-wasm.',
};
/**
 * Biome matches specifiers, not resolved paths: `./.././../x` would slip past the depth rules.
 * Rejected everywhere. (`../a/../../x` is not expressible as a Biome glob; tsc rejects it, see README.)
 */
export const NO_UNNORMALIZED = {
  group: ['./**/../**', './**/..'],
  message: 'ADR-005: write relative imports normalized (no ".." after "./"); a non-normalized path hides where it goes.',
};
export const ESCAPE_MESSAGE = 'ADR-005: no relative escapes out of the package; import its public entry point.';
export const DEEP_ESCAPE_MESSAGE =
  ESCAPE_MESSAGE + ' (files deeper than three subfolders: no import may climb four or more levels)';

/** Only rumbo packages listed in `rumbo` may be imported (Biome) and declared (package.json). */
function onlyRumbo(rumbo, message) {
  return { group: ['@rumbo/**', ...rumbo.map((name) => '!' + name)], message };
}

/**
 * TypeScript workspace packages, keyed by package name.
 * - `layout: 'flat'`: every source file sits at the package root; one override rejects any `../`.
 * - `layout: 'nested'`: overrides per folder depth reject only the `../` that leave the package.
 * - `isolatedSubfolders`: those subfolders may not import each other nor the package root.
 * - `rumbo`: the only @rumbo packages it may import and declare as dependencies.
 * - `rejected`/`allowed`: guardrail cases (allowed must lint clean, each rejected must fail).
 * - `tooling`/`generated`: no import rules of their own; the reason says why.
 */
export const TS = {
  '@rumbo/contracts': {
    dir: 'packages/contracts',
    exts: '{ts,tsx}',
    layout: 'flat',
    rumbo: [],
    forbid: [
      NO_UI_OR_RENDER,
      {
        group: ['@rumbo/**'],
        message:
          'ADR-005: contracts imports no other @rumbo package; invert the dependency through a port declared here.',
      },
    ],
    allowed: 'import type { SimPort } from "./ports.js"; export type { SimPort };',
    rejected: [
      'import type { X } from "react";',
      'import "three/examples/test";',
      'const load = () => import("@rumbo/game");',
      'import "react-dom/client";',
      'import "@rumbo/sim-wasm";',
    ],
  },
  '@rumbo/sim-worker': {
    dir: 'packages/sim-worker',
    exts: '{ts,tsx}',
    layout: 'flat',
    rumbo: ['@rumbo/contracts', '@rumbo/sim-wasm'],
    forbid: [
      NO_UI_OR_RENDER,
      onlyRumbo(
        ['@rumbo/contracts', '@rumbo/sim-wasm'],
        'ADR-005: sim-worker may import only @rumbo/contracts and @rumbo/sim-wasm.',
      ),
    ],
    allowed:
      'import type { SimPort } from "@rumbo/contracts"; import init from "@rumbo/sim-wasm"; export { init }; export type { SimPort };',
    rejected: [
      'import type { X } from "react";',
      'import "three/examples/test";',
      'const load = () => import("@rumbo/game");',
      'import "react-dom/client";',
    ],
  },
  '@rumbo/render-three': {
    dir: 'packages/render-three',
    exts: '{ts,tsx,mjs}',
    layout: 'nested',
    rumbo: ['@rumbo/contracts'],
    forbid: [
      NO_REACT,
      onlyRumbo(
        ['@rumbo/contracts'],
        'ADR-005: render-three may import only @rumbo/contracts (and three); apps/web wires it to the other adapters.',
      ),
    ],
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
  '@rumbo/platform-web': {
    dir: 'packages/platform-web',
    exts: '{ts,tsx,mjs}',
    layout: 'nested',
    rumbo: ['@rumbo/contracts'],
    isolatedSubfolders: ['audio', 'input', 'storage', 'telemetry', 'zone-source'],
    forbid: [
      NO_REACT,
      NO_THREE,
      onlyRumbo(
        ['@rumbo/contracts'],
        'ADR-005: platform-web may import only @rumbo/contracts; apps/web wires it to the other adapters.',
      ),
    ],
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
  '@rumbo/ui': {
    dir: 'packages/ui',
    exts: '{ts,tsx,mjs}',
    layout: 'nested',
    rumbo: ['@rumbo/contracts', '@rumbo/game'],
    forbid: [
      NO_THREE,
      onlyRumbo(
        ['@rumbo/contracts', '@rumbo/game'],
        'ADR-005: ui may import only @rumbo/contracts (and @rumbo/game); adapters reach it as ports injected by mountUi.',
      ),
    ],
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
  '@rumbo/devtools': {
    dir: 'packages/devtools',
    exts: '{ts,tsx,mjs}',
    layout: 'nested',
    rumbo: ['@rumbo/contracts', '@rumbo/game'],
    forbid: [
      NO_THREE,
      onlyRumbo(
        ['@rumbo/contracts', '@rumbo/game'],
        'ADR-005: devtools may import only @rumbo/contracts (and @rumbo/game); the renderer reaches it as RenderDebugPort injected by mountDevtools.',
      ),
    ],
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
  '@rumbo/web': {
    dir: 'apps/web',
    exts: '{ts,tsx}',
    layout: 'nested',
    compositionRoot: true,
    rumbo: [
      '@rumbo/contracts',
      '@rumbo/sim-worker',
      '@rumbo/render-three',
      '@rumbo/platform-web',
      '@rumbo/ui',
      '@rumbo/devtools',
    ],
    forbid: [],
    escapeMessage:
      'ADR-005: apps/web imports packages by name (@rumbo/*), never by relative path into packages/; assets go through the @assets alias.',
    allowed:
      'import { createWorkerSim } from "@rumbo/sim-worker"; import { createThreeRenderer } from "@rumbo/render-three"; import { createWebPlatform } from "@rumbo/platform-web"; import { mountUi } from "@rumbo/ui"; import type { SimPort } from "@rumbo/contracts"; const dev = () => import("@rumbo/devtools"); import { tracks } from "../tracks.ts"; export { createWorkerSim, createThreeRenderer, createWebPlatform, mountUi, dev, tracks }; export type { SimPort };',
    rejected: [],
  },
  '@rumbo/sim-wasm': {
    dir: 'crates/sim-wasm',
    generated: 'wasm-pack output of the sim-wasm crate; its layers are enforced by Cargo (see RUST).',
    rumbo: [],
  },
  'rumbo-determinism': {
    dir: 'ci/determinism',
    tooling: 'CI harness that re-simulates goldens in browsers; never imported by product code.',
    rumbo: ['@rumbo/sim-wasm'],
  },
  'exact-versions': {
    dir: 'ci/exact-versions',
    tooling: 'CI check for exact dependency pins; never imported by product code.',
    rumbo: [],
  },
  'rumbo-layers': {
    dir: 'ci/layers',
    tooling: 'This policy, its Biome generator, the meta-check and the guardrail tests.',
    rumbo: [],
  },
};

/**
 * Cargo members, keyed by crate name.
 * - `internal`: workspace crates allowed as normal/build dependencies.
 * - `devInternal`: workspace crates allowed only as dev-dependencies.
 * - `core`: deterministic core (P1); gets the canonical core clippy.toml.
 */
export const RUST = {
  'track-format': { layer: 'kernel', internal: [], core: true },
  'sim-core': { layer: 'core', internal: ['track-format'], core: true },
  'track-compiler-core': { layer: 'core', internal: ['track-format'], core: true },
  'sim-wasm': { layer: 'adapter', internal: ['track-format', 'sim-core'] },
  'compiler-cli': { layer: 'adapter', internal: ['track-compiler-core'], devInternal: ['sim-core'] },
};

/** Crates that must never enter the dependency graph (normal/build) of a core crate (P1). */
export const CORE_FORBIDDEN_CRATES = ['rand', 'rand_core', 'rand_chacha', 'getrandom', 'fastrand', 'chrono', 'instant', 'web-time'];

/**
 * Folder depths (0 … NESTED_DEPTHS - 1) that get their own escape override in nested packages.
 * Deeper files share one override that rejects climbing NESTED_DEPTHS + 1 levels.
 */
export const NESTED_DEPTHS = 3;
