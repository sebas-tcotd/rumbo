#!/usr/bin/env node
// Meta-check of the layer policy (ADR-005): fails (exit 1) when a pnpm package or a Cargo member has
// no declared layer rule, when declared dependencies or project references go beyond the policy,
// when a crate does not inherit the workspace lints, when a core crate lacks the canonical
// clippy.toml, when a Biome message does not cite an ADR, or when biome.json is out of sync.
//
// Packages come from pnpm-workspace.yaml and crates from `cargo metadata`, never from walking the
// disk (worktrees under .claude/ hold copies of the packages). Pure Node, no npm dependencies.

import { execFileSync } from 'node:child_process';
import { existsSync, readdirSync, readFileSync } from 'node:fs';
import { join, relative, resolve } from 'node:path';
import { fileURLToPath, pathToFileURL } from 'node:url';
import { biomeConfig, isInSync } from './biome.mjs';
import { CORE_FORBIDDEN_CRATES, RUST, TS } from './policy.mjs';

const ROOT = fileURLToPath(new URL('../../', import.meta.url));
export const CORE_CLIPPY = 'ci/layers/core-clippy.toml';
const ADR = /ADR-\d{3}/;

/** Globs under `packages:` in pnpm-workspace.yaml (minimal parser: `  - item` lines). */
export function workspaceGlobs(yaml) {
  const lines = yaml.replace(/\r\n/g, '\n').split('\n');
  const start = lines.findIndex((line) => /^packages:\s*$/.test(line));
  if (start < 0) throw new Error('pnpm-workspace.yaml has no packages: list');
  const globs = [];
  for (const line of lines.slice(start + 1)) {
    const item = /^\s+-\s+['"]?([^'"#\s]+)['"]?\s*$/.exec(line);
    if (!item) break;
    globs.push(item[1]);
  }
  return globs;
}

/** Package directories (relative, forward slashes) matched by `dir` or `dir/*` globs. */
export function expandGlobs(root, globs) {
  const dirs = [];
  for (const glob of globs) {
    if (!glob.endsWith('/*')) {
      dirs.push(glob);
      continue;
    }
    const base = glob.slice(0, -2);
    for (const entry of readdirSync(join(root, base), { withFileTypes: true })) {
      if (entry.isDirectory()) dirs.push(base + '/' + entry.name);
    }
  }
  return dirs.filter((dir) => existsSync(join(root, dir, 'package.json')));
}

function readJson(path) {
  return JSON.parse(readFileSync(path, 'utf8'));
}

/** What the TS check needs to know about one workspace package. */
export function describePackage(root, dir) {
  const pkg = readJson(join(root, dir, 'package.json'));
  const tsconfigPath = join(root, dir, 'tsconfig.json');
  const tsconfig = existsSync(tsconfigPath) ? readJson(tsconfigPath) : null;
  const references = (tsconfig?.references ?? []).map((ref) =>
    relative(root, resolve(root, dir, ref.path)).replaceAll('\\', '/'),
  );
  const deps = { ...pkg.dependencies, ...pkg.devDependencies, ...pkg.optionalDependencies };
  const rumboDeps = Object.keys(deps).filter((name) => name.startsWith('@rumbo/'));
  return { name: pkg.name, dir, rumboDeps, references, hasTsconfig: tsconfig !== null };
}

/** Errors for one package against its policy entry. `byName` maps package names to descriptions. */
function checkPackage(info, layer, byName) {
  const errors = [];
  if (layer.dir !== info.dir) errors.push(`${info.name}: policy dir ${layer.dir} but package lives in ${info.dir}`);
  for (const dep of info.rumboDeps) {
    if (!layer.rumbo.includes(dep)) errors.push(`${info.name}: depends on ${dep}, which its layer may not import`);
  }
  if (!info.hasTsconfig) return errors;
  const expected = info.rumboDeps.filter((dep) => byName.get(dep)?.hasTsconfig).map((dep) => byName.get(dep).dir);
  for (const dir of expected) {
    if (!info.references.includes(dir)) errors.push(`${info.name}: tsconfig is missing the project reference ${dir}`);
  }
  for (const dir of info.references) {
    if (!expected.includes(dir)) errors.push(`${info.name}: tsconfig references ${dir}, which is not a declared @rumbo dependency`);
  }
  return errors;
}

/** Errors for the TS side: every package declared, dependencies and references inside the policy. */
export function checkTs(packages, policy = TS) {
  const errors = [];
  const byName = new Map(packages.map((info) => [info.name, info]));
  for (const info of packages) {
    const layer = policy[info.name];
    if (!layer) errors.push(`${info.name} (${info.dir}): no layer rule in ci/layers/policy.mjs`);
    else errors.push(...checkPackage(info, layer, byName));
  }
  for (const name of Object.keys(policy)) {
    if (!byName.has(name)) errors.push(`${name}: listed in ci/layers/policy.mjs but not a workspace package`);
  }
  return errors;
}

/** Errors when a Biome message does not say which ADR it enforces. */
export function checkMessages(config) {
  const errors = [];
  for (const entry of config.overrides) {
    for (const { message } of entry.linter.rules.style.noRestrictedImports.options.patterns) {
      if (!ADR.test(message)) errors.push(`${entry.includes[0]}: Biome message does not cite an ADR: ${message}`);
    }
  }
  return errors;
}

/** Workspace crates a member depends on, split into normal/build and dev edges. */
function internalDeps(pkg, members) {
  const normal = [];
  const dev = [];
  for (const dep of pkg.dependencies) {
    if (!members.has(dep.name)) continue;
    (dep.kind === 'dev' ? dev : normal).push(dep.name);
  }
  return { normal, dev };
}

/** Does the manifest opt into the workspace lints? */
export function inheritsLints(manifest) {
  return /^\[lints\]\s*\r?\n\s*workspace\s*=\s*true\s*$/m.test(manifest);
}

/** Errors for one crate against its policy entry. */
function checkCrate(pkg, rule, members, files) {
  const errors = [];
  const { normal, dev } = internalDeps(pkg, members);
  for (const dep of normal) {
    if (!rule.internal.includes(dep)) errors.push(`${pkg.name}: depends on ${dep}, which its layer may not use`);
  }
  for (const dep of dev) {
    const allowed = [...rule.internal, ...(rule.devInternal ?? [])];
    if (!allowed.includes(dep)) errors.push(`${pkg.name}: dev-depends on ${dep}, which its layer may not use`);
  }
  if (!inheritsLints(files.manifest)) errors.push(`${pkg.name}: Cargo.toml needs [lints] workspace = true`);
  if (rule.core && (files.canonical === null || files.clippy !== files.canonical)) {
    errors.push(`${pkg.name}: clippy.toml must be a copy of ${CORE_CLIPPY} (core crate, P1)`);
  }
  return errors;
}

/** Package ids reachable from `start` through normal and build edges of the resolve graph. */
function reachable(metadata, start) {
  const nodes = new Map(metadata.resolve.nodes.map((node) => [node.id, node]));
  const seen = new Set();
  const stack = [start];
  while (stack.length) {
    const id = stack.pop();
    if (seen.has(id)) continue;
    seen.add(id);
    for (const dep of nodes.get(id)?.deps ?? []) {
      if (dep.dep_kinds.some((kind) => kind.kind !== 'dev')) stack.push(dep.pkg);
    }
  }
  return seen;
}

/** Errors when a core crate pulls randomness or wall-clock crates into its graph (P1). */
export function checkCoreGraph(metadata, policy = RUST, forbidden = CORE_FORBIDDEN_CRATES) {
  const names = new Map(metadata.packages.map((pkg) => [pkg.id, pkg.name]));
  const errors = [];
  for (const pkg of metadata.packages) {
    if (!policy[pkg.name]?.core || !metadata.workspace_members.includes(pkg.id)) continue;
    for (const id of reachable(metadata, pkg.id)) {
      const name = names.get(id);
      if (forbidden.includes(name)) errors.push(`${pkg.name}: ${name} in its dependency graph (P1: no randomness or wall clock)`);
    }
  }
  return errors;
}

/** Errors for the Rust side. `files(name)` returns { manifest, clippy, canonical } texts. */
export function checkRust(metadata, files, policy = RUST) {
  const members = metadata.packages.filter((pkg) => metadata.workspace_members.includes(pkg.id));
  const names = new Set(members.map((pkg) => pkg.name));
  const errors = [];
  for (const pkg of members) {
    const rule = policy[pkg.name];
    if (!rule) errors.push(`${pkg.name}: Cargo member with no layer rule in ci/layers/policy.mjs`);
    else errors.push(...checkCrate(pkg, rule, names, files(pkg)));
  }
  for (const name of Object.keys(policy)) {
    if (!names.has(name)) errors.push(`${name}: listed in ci/layers/policy.mjs but not a Cargo member`);
  }
  return [...errors, ...checkCoreGraph(metadata, policy)];
}

function readText(path) {
  return existsSync(path) ? readFileSync(path, 'utf8').replace(/\r\n/g, '\n') : null;
}

/** Runs every check against the repo at `root` and returns the errors. */
export function check(root = ROOT) {
  const dirs = expandGlobs(root, workspaceGlobs(readFileSync(join(root, 'pnpm-workspace.yaml'), 'utf8')));
  const packages = dirs.map((dir) => describePackage(root, dir));
  const metadata = JSON.parse(
    execFileSync('cargo', ['metadata', '--format-version', '1', '--locked'], {
      cwd: root,
      encoding: 'utf8',
      maxBuffer: 64 * 1024 * 1024,
    }),
  );
  const canonical = readText(join(root, CORE_CLIPPY));
  const files = (pkg) => {
    const dir = resolve(pkg.manifest_path, '..');
    return { manifest: readText(pkg.manifest_path), clippy: readText(join(dir, 'clippy.toml')), canonical };
  };
  const errors = [...checkTs(packages), ...checkRust(metadata, files), ...checkMessages(biomeConfig())];
  if (!isInSync(join(root, 'biome.json'))) errors.push('biome.json is out of sync; run `node ci/layers/biome.mjs`');
  return errors;
}

if (process.argv[1] && import.meta.url === pathToFileURL(process.argv[1]).href) {
  const errors = check();
  for (const error of errors) console.error('layers: ' + error);
  if (errors.length) process.exit(1);
  console.log('layers: every package and crate has a declared layer rule');
}
