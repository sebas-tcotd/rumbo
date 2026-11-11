#!/usr/bin/env node
// Generates biome.json from the layer policy (policy.mjs).
//
//   node ci/layers/biome.mjs           rewrite biome.json
//   node ci/layers/biome.mjs --check   exit 1 if biome.json differs from the generated file
//
// Biome compares import specifiers, not resolved paths, and a later override that matches a file
// replaces the whole `noRestrictedImports` options of the earlier ones. So every override carries
// the full list of the package's prohibitions, and relative escapes are written per folder depth:
// a file `d` folders below the package root may not climb `d + 1` levels.

import { readFileSync, writeFileSync } from 'node:fs';
import { join } from 'node:path';
import { fileURLToPath, pathToFileURL } from 'node:url';
import { DEEP_ESCAPE_MESSAGE, ESCAPE_MESSAGE, NESTED_DEPTHS, NO_UNNORMALIZED, NO_WASM, TS } from './policy.mjs';

const ROOT = fileURLToPath(new URL('../../', import.meta.url));
export const BIOME_PATH = join(ROOT, 'biome.json');

/** Globs Biome lints. Apps join packages (story 0.7); build outputs are always skipped. */
const FILES = [
  'packages/**/*.{ts,tsx,mjs}',
  'apps/**/*.{ts,tsx}',
  'ci/determinism/{worker-host.mjs,worker-integration.test.mjs,lab-smoke.mjs}',
  '!**/dist',
  '!**/dist-lab',
  '!**/node_modules',
  '!**/pkg',
  '!**/.turbo',
  // Never scanned: worktrees under .claude/ carry their own biome.json (a nested root config).
  '!!.claude',
];

/** `../` repeated `levels` times followed by `**`. */
export function climb(levels) {
  return '../'.repeat(levels) + '**';
}

/** One override: the given files get the package prohibitions plus one escape rule and the WASM rule. */
function override(includes, forbid, escape) {
  const patterns = [...forbid, escape, NO_UNNORMALIZED, NO_WASM].map(({ group, message }) => ({ group: [...group], message }));
  return {
    includes: [includes],
    linter: { rules: { style: { noRestrictedImports: { level: 'error', options: { patterns } } } } },
  };
}

/** `*\/` repeated `depth` times: the folder part of a glob for files exactly `depth` folders deep. */
function folders(depth) {
  return '*/'.repeat(depth);
}

/** Overrides for one package with source in nested folders. */
function nestedOverrides(layer) {
  const { dir, exts, forbid } = layer;
  const message = layer.escapeMessage ?? ESCAPE_MESSAGE;
  const deep = layer.escapeMessage ?? DEEP_ESCAPE_MESSAGE;
  const list = [override(`${dir}/**/*.${exts}`, forbid, { group: [climb(NESTED_DEPTHS + 1)], message: deep })];
  for (let depth = 0; depth < NESTED_DEPTHS; depth++) {
    list.push(override(`${dir}/${folders(depth)}*.${exts}`, forbid, { group: [climb(depth + 1)], message }));
  }
  for (const sub of layer.isolatedSubfolders ?? []) list.push(...subfolderOverrides(layer, sub));
  return list;
}

/** A subfolder `d` folders deep may not climb `d` levels: that would leave the subfolder. */
function subfolderOverrides({ dir, exts, forbid }, sub) {
  const name = dir.split('/').pop();
  const message = `ADR-005: ${name} subfolders are isolated adapters; ${sub}/ must not import other subfolders or the package root. Share types through @rumbo/contracts; apps/web wires them together.`;
  const list = [override(`${dir}/${sub}/**/*.${exts}`, forbid, { group: [climb(NESTED_DEPTHS)], message })];
  for (let depth = 1; depth < NESTED_DEPTHS; depth++) {
    list.push(override(`${dir}/${sub}/${folders(depth - 1)}*.${exts}`, forbid, { group: [climb(depth)], message }));
  }
  return list;
}

/** Overrides for every package with import rules, in policy order. */
export function overrides(policy = TS) {
  const list = [];
  for (const layer of Object.values(policy)) {
    if (layer.tooling || layer.generated) continue;
    if (layer.layout === 'flat') {
      list.push(override(`${layer.dir}/**/*.${layer.exts}`, layer.forbid, { group: ['../**'], message: ESCAPE_MESSAGE }));
    } else list.push(...nestedOverrides(layer));
  }
  return list;
}

/** The full biome.json document. */
export function biomeConfig(policy = TS) {
  return {
    files: { includes: FILES },
    linter: { rules: { preset: 'none' } },
    overrides: overrides(policy),
    formatter: { indentStyle: 'space', indentWidth: 2, lineWidth: 110, lineEnding: 'lf' },
    javascript: { formatter: { quoteStyle: 'single' } },
  };
}

/** biome.json text as written to disk (LF, trailing newline). */
export function biomeText(policy = TS) {
  return JSON.stringify(biomeConfig(policy), null, 2) + '\n';
}

/** Does the file on disk match the generated config? Line endings are ignored (core.autocrlf). */
export function isInSync(path = BIOME_PATH, policy = TS) {
  return readFileSync(path, 'utf8').replace(/\r\n/g, '\n') === biomeText(policy);
}

if (process.argv[1] && import.meta.url === pathToFileURL(process.argv[1]).href) {
  if (process.argv.includes('--check')) {
    if (!isInSync()) {
      console.error('biome.json is out of sync with ci/layers/policy.mjs; run `node ci/layers/biome.mjs`.');
      process.exit(1);
    }
    console.log('biome.json matches ci/layers/policy.mjs');
  } else {
    writeFileSync(BIOME_PATH, biomeText());
    console.log('biome.json regenerated from ci/layers/policy.mjs');
  }
}
