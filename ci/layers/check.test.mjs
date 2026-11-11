import assert from 'node:assert/strict';
import { mkdirSync, mkdtempSync, rmSync, writeFileSync } from 'node:fs';
import { tmpdir } from 'node:os';
import { join } from 'node:path';
import test from 'node:test';
import { biomeConfig } from './biome.mjs';
import {
  checkCoreGraph,
  checkMessages,
  checkRust,
  checkTs,
  describePackage,
  expandGlobs,
  inheritsLints,
  workspaceGlobs,
} from './check.mjs';

const POLICY = {
  '@rumbo/contracts': { dir: 'packages/contracts', rumbo: [] },
  '@rumbo/ui': { dir: 'packages/ui', rumbo: ['@rumbo/contracts'] },
  '@rumbo/sim-wasm': { dir: 'crates/sim-wasm', generated: 'wasm-pack', rumbo: [] },
};
const contracts = { name: '@rumbo/contracts', dir: 'packages/contracts', rumboDeps: [], references: [], hasTsconfig: true };
const ui = (over = {}) => ({
  name: '@rumbo/ui',
  dir: 'packages/ui',
  rumboDeps: ['@rumbo/contracts'],
  references: ['packages/contracts'],
  hasTsconfig: true,
  ...over,
});
const simWasm = { name: '@rumbo/sim-wasm', dir: 'crates/sim-wasm', rumboDeps: [], references: [], hasTsconfig: false };

test('a consistent TS workspace passes', () => {
  assert.deepEqual(checkTs([contracts, ui(), simWasm], POLICY), []);
});

test('a package without a layer rule fails', () => {
  const extra = { name: '@rumbo/game', dir: 'packages/game', rumboDeps: [], references: [], hasTsconfig: true };
  assert.match(checkTs([contracts, ui(), simWasm, extra], POLICY).join('\n'), /@rumbo\/game .*no layer rule/);
});

test('a policy entry for a package that no longer exists fails', () => {
  assert.match(checkTs([contracts, ui()], POLICY).join('\n'), /@rumbo\/sim-wasm: listed .* not a workspace package/);
});

test('a forbidden @rumbo dependency fails', () => {
  const bad = ui({ rumboDeps: ['@rumbo/contracts', '@rumbo/sim-wasm'] });
  assert.match(checkTs([contracts, bad, simWasm], POLICY).join('\n'), /depends on @rumbo\/sim-wasm/);
});

test('a missing project reference fails', () => {
  assert.match(checkTs([contracts, ui({ references: [] }), simWasm], POLICY).join('\n'), /missing the project reference/);
});

test('a project reference without the dependency fails', () => {
  const bad = ui({ rumboDeps: [], references: ['packages/contracts'] });
  assert.match(checkTs([contracts, bad, simWasm], POLICY).join('\n'), /not a declared @rumbo dependency/);
});

test('a dependency on a non-TS package needs no reference', () => {
  const policy = { ...POLICY, '@rumbo/ui': { dir: 'packages/ui', rumbo: ['@rumbo/contracts', '@rumbo/sim-wasm'] } };
  const withWasm = ui({ rumboDeps: ['@rumbo/contracts', '@rumbo/sim-wasm'] });
  assert.deepEqual(checkTs([contracts, withWasm, simWasm], policy), []);
});

test('a package that moved folders fails', () => {
  assert.match(checkTs([contracts, ui({ dir: 'packages/hud' }), simWasm], POLICY).join('\n'), /policy dir packages\/ui/);
});

test('every generated Biome message cites an ADR, and a message without one fails', () => {
  assert.deepEqual(checkMessages(biomeConfig()), []);
  const config = biomeConfig();
  config.overrides[0].linter.rules.style.noRestrictedImports.options.patterns[0].message = 'do not';
  assert.match(checkMessages(config).join('\n'), /does not cite an ADR/);
});

const LINTS = '[package]\nname = "x"\n\n[lints]\nworkspace = true\n';
const RULES = {
  'track-format': { internal: [], core: true },
  'sim-core': { internal: ['track-format'], core: true },
  'compiler-cli': { internal: [], devInternal: ['sim-core'] },
};

/** cargo metadata with the given members; `deps` maps crate → [[name, kind]]. */
function metadata(deps, extra = []) {
  const names = [...Object.keys(deps), ...extra];
  const packages = names.map((name) => ({
    id: name,
    name,
    manifest_path: name,
    dependencies: (deps[name] ?? []).map(([dep, kind]) => ({ name: dep, kind })),
  }));
  const nodes = names.map((name) => ({
    id: name,
    deps: (deps[name] ?? []).map(([dep, kind]) => ({ pkg: dep, dep_kinds: [{ kind }] })),
  }));
  return { packages, workspace_members: Object.keys(deps), resolve: { nodes } };
}
const GOOD = { 'track-format': [], 'sim-core': [['track-format', null]], 'compiler-cli': [['sim-core', 'dev']] };
const files = (over = {}) => () => ({ manifest: LINTS, clippy: 'core', canonical: 'core', ...over });

test('a consistent Cargo workspace passes', () => {
  assert.deepEqual(checkRust(metadata(GOOD), files(), RULES), []);
});

test('a crate without a layer rule fails', () => {
  const errors = checkRust(metadata({ ...GOOD, 'route-compiler-wasm': [] }), files(), RULES);
  assert.match(errors.join('\n'), /route-compiler-wasm: Cargo member with no layer rule/);
});

test('a policy entry for a crate that is not a member fails', () => {
  const { 'compiler-cli': _, ...rest } = GOOD;
  assert.match(checkRust(metadata(rest), files(), RULES).join('\n'), /compiler-cli: listed .* not a Cargo member/);
});

test('a normal dependency outside the layer fails, a dev one allowed by devInternal passes', () => {
  const bad = { ...GOOD, 'compiler-cli': [['sim-core', null]] };
  assert.match(checkRust(metadata(bad), files(), RULES).join('\n'), /compiler-cli: depends on sim-core/);
});

test('a crate without [lints] workspace = true fails', () => {
  assert.match(checkRust(metadata(GOOD), files({ manifest: '[package]\n' }), RULES).join('\n'), /\[lints\] workspace = true/);
  assert.equal(inheritsLints('[lints]\r\nworkspace = true\r\n'), true);
  assert.equal(inheritsLints('[lints]\nworkspace = false\n'), false);
});

test('a core crate with a missing or different clippy.toml fails', () => {
  assert.match(checkRust(metadata(GOOD), files({ clippy: null }), RULES).join('\n'), /sim-core: clippy.toml must be a copy/);
  assert.match(checkRust(metadata(GOOD), files({ clippy: 'other' }), RULES).join('\n'), /track-format: clippy.toml/);
  assert.match(checkRust(metadata(GOOD), files({ clippy: null, canonical: null }), RULES).join('\n'), /clippy.toml must be a copy/);
});

test('rand or a wall-clock crate in a core graph fails, a dev-only edge does not', () => {
  const withRand = { ...GOOD, 'sim-core': [['track-format', null], ['rand', null]] };
  assert.match(checkCoreGraph(metadata(withRand, ['rand']), RULES).join('\n'), /sim-core: rand in its dependency graph/);
  const transitive = { ...GOOD, 'track-format': [['getrandom', 'build']] };
  assert.match(checkCoreGraph(metadata(transitive, ['getrandom']), RULES).join('\n'), /getrandom/);
  const devOnly = { ...GOOD, 'sim-core': [['track-format', null], ['rand', 'dev']] };
  assert.deepEqual(checkCoreGraph(metadata(devOnly, ['rand']), RULES), []);
});

test('workspace globs and package discovery', () => {
  const yaml = 'packages:\r\n  - apps/*\r\n  - "crates/sim-wasm"\r\n\r\nminimumReleaseAge: 1\r\n';
  assert.deepEqual(workspaceGlobs(yaml), ['apps/*', 'crates/sim-wasm']);
  assert.throws(() => workspaceGlobs('catalog:\n'), /no packages/);
  const root = mkdtempSync(join(tmpdir(), 'layers-'));
  try {
    for (const dir of ['apps/web', 'apps/notes', 'crates/sim-wasm']) mkdirSync(join(root, dir), { recursive: true });
    writeFileSync(join(root, 'apps/web/package.json'), JSON.stringify({ name: '@rumbo/web', dependencies: { '@rumbo/ui': 'workspace:*', react: '1.0.0' } }));
    writeFileSync(join(root, 'apps/web/tsconfig.json'), JSON.stringify({ references: [{ path: '../../packages/ui' }] }));
    writeFileSync(join(root, 'crates/sim-wasm/package.json'), '{"name":"@rumbo/sim-wasm"}');
    assert.deepEqual(expandGlobs(root, ['apps/*', 'crates/sim-wasm']), ['apps/web', 'crates/sim-wasm']);
    const web = describePackage(root, 'apps/web');
    assert.deepEqual(web, { name: '@rumbo/web', dir: 'apps/web', rumboDeps: ['@rumbo/ui'], references: ['packages/ui'], hasTsconfig: true });
  } finally {
    rmSync(root, { recursive: true, force: true });
  }
});
