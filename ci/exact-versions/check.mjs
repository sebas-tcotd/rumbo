#!/usr/bin/env node
// Fails (exit 1) when a direct dependency uses a version range instead of an exact version.
//
// npm:   every package.json in the repo (dependencies, devDependencies, optionalDependencies).
//        Exact = full semver `x.y.z` (optional prerelease/build), `workspace:*` or `catalog:`.
//        peerDependencies and engines are ranges by nature and are not checked.
// Cargo: every workspace member (via `cargo metadata --no-deps`) plus the root
//        `[workspace.dependencies]` table, which also holds pins nobody consumes yet.
//        Exact = `=x.y.z`. A dependency with only a `path` (no version) is allowed.
//
// Pure Node, no npm dependencies. Run from anywhere: `node ci/exact-versions/check.mjs`.

import { execFileSync } from 'node:child_process';
import { readdirSync, readFileSync } from 'node:fs';
import path from 'node:path';
import { fileURLToPath, pathToFileURL } from 'node:url';

const SEMVER = /^\d+\.\d+\.\d+(?:-[0-9A-Za-z-]+(?:\.[0-9A-Za-z-]+)*)?(?:\+[0-9A-Za-z-]+(?:\.[0-9A-Za-z-]+)*)?$/;

export const NPM_DEP_FIELDS = ['dependencies', 'devDependencies', 'optionalDependencies'];

/** Directory names never walked when looking for package.json files. */
export const EXCLUDED_DIRS = new Set(['node_modules', 'target', 'pkg', 'dist']);

/** Is an npm version specifier an exact pin? */
export function isExactNpmSpec(spec) {
  if (typeof spec !== 'string') return false;
  if (spec === 'workspace:*') return true;
  if (/^catalog:[\w@/.-]*$/.test(spec)) return true;
  return SEMVER.test(spec);
}

/** Is a Cargo version requirement an exact pin (`=x.y.z`)? */
export function isExactCargoReq(req) {
  return typeof req === 'string' && req.startsWith('=') && SEMVER.test(req.slice(1).trim());
}

/**
 * Classifies one Cargo dependency. `req` is the version requirement (cargo metadata reports
 * `*` when none was written); a dependency with a `path` and no version is allowed.
 */
export function isAllowedCargoDep({ req, path: depPath }) {
  if (depPath && (req === undefined || req === null || req === '*')) return true;
  return isExactCargoReq(req);
}

/** Returns the violations in one parsed package.json. */
export function checkPackageJson(pkg, file) {
  const violations = [];
  for (const field of NPM_DEP_FIELDS) {
    for (const [name, spec] of Object.entries(pkg?.[field] ?? {})) {
      if (!isExactNpmSpec(spec)) violations.push({ file, dependency: `${name} (${field})`, version: String(spec) });
    }
  }
  return violations;
}

/** Returns the violations in a `cargo metadata --format-version 1 --no-deps` result. */
export function checkCargoMetadata(metadata, root) {
  const violations = [];
  for (const pkg of metadata.packages ?? []) {
    const file = root ? path.relative(root, pkg.manifest_path) : pkg.manifest_path;
    for (const dep of pkg.dependencies ?? []) {
      if (!isAllowedCargoDep(dep)) {
        const kind = dep.kind ? `${dep.kind}-dependencies` : 'dependencies';
        violations.push({ file, dependency: `${dep.name} (${kind})`, version: dep.req });
      }
    }
  }
  return violations;
}

const unquote = (s) => s.trim().replace(/^"(.*)"$/, '$1');

/**
 * Minimal line-based reader of the `[workspace.dependencies]` table of a Cargo.toml.
 * Handles `name = "ver"`, `name = { version = "ver", ... }` (one line) and
 * `[workspace.dependencies.name]` sub-tables. Returns `[{ name, req, path }]`.
 */
export function parseWorkspaceDependencies(toml) {
  const deps = [];
  let section = null; // 'table' | { sub: dep }
  for (const raw of toml.split(/\r?\n/)) {
    const line = raw.replace(/(^|\s)#.*$/, '').trim();
    if (!line) continue;
    const header = line.match(/^\[([^\]]+)\]$/);
    if (header) {
      const name = header[1].trim();
      if (name === 'workspace.dependencies') section = 'table';
      else if (name.startsWith('workspace.dependencies.')) {
        const dep = { name: unquote(name.slice('workspace.dependencies.'.length)), req: undefined, path: undefined };
        deps.push(dep);
        section = { sub: dep };
      } else section = null;
      continue;
    }
    if (!section) continue;
    const kv = line.match(/^("[^"]+"|[\w-]+)\s*=\s*(.+)$/);
    if (!kv) continue;
    const key = unquote(kv[1]);
    const value = kv[2].trim();
    if (section === 'table') {
      if (value.startsWith('{')) {
        const version = value.match(/\bversion\s*=\s*"([^"]*)"/);
        const depPath = value.match(/\bpath\s*=\s*"([^"]*)"/);
        deps.push({ name: key, req: version?.[1], path: depPath?.[1] });
      } else {
        deps.push({ name: key, req: unquote(value), path: undefined });
      }
    } else if (key === 'version') section.sub.req = unquote(value);
    else if (key === 'path') section.sub.path = unquote(value);
  }
  return deps;
}

/** Returns the violations in the root `[workspace.dependencies]` table. */
export function checkWorkspaceDependencies(toml, file) {
  return parseWorkspaceDependencies(toml)
    .filter((dep) => !isAllowedCargoDep(dep))
    .map((dep) => ({ file, dependency: `${dep.name} (workspace.dependencies)`, version: String(dep.req) }));
}

/** Finds every package.json under `root`, skipping build/vendor dirs and `excludedPaths`. */
export function findPackageJsons(root, excludedPaths = []) {
  const excluded = excludedPaths.map((p) => path.resolve(p));
  const found = [];
  const walk = (dir) => {
    for (const entry of readdirSync(dir, { withFileTypes: true })) {
      const full = path.join(dir, entry.name);
      if (entry.isDirectory()) {
        if (EXCLUDED_DIRS.has(entry.name) || entry.name.startsWith('.')) continue;
        if (excluded.includes(path.resolve(full))) continue;
        walk(full);
      } else if (entry.name === 'package.json') {
        found.push(full);
      }
    }
  };
  walk(root);
  return found.sort();
}

/** Runs every check against the repo at `root`. */
export function checkRepo(root, { excludedPaths = [], cargoMetadata } = {}) {
  const violations = [];
  const packageJsons = findPackageJsons(root, excludedPaths);
  for (const file of packageJsons) {
    const pkg = JSON.parse(readFileSync(file, 'utf8'));
    violations.push(...checkPackageJson(pkg, path.relative(root, file)));
  }

  let cargoMembers = 0;
  const cargoToml = path.join(root, 'Cargo.toml');
  let toml = null;
  try {
    toml = readFileSync(cargoToml, 'utf8');
  } catch {
    // No Cargo workspace here: nothing to check on the Rust side.
  }
  if (toml !== null) {
    const metadata =
      cargoMetadata ??
      JSON.parse(
        execFileSync('cargo', ['metadata', '--format-version', '1', '--no-deps', '--manifest-path', cargoToml], {
          encoding: 'utf8',
          maxBuffer: 64 * 1024 * 1024,
        }),
      );
    cargoMembers = metadata.packages?.length ?? 0;
    violations.push(...checkCargoMetadata(metadata, root));
    violations.push(...checkWorkspaceDependencies(toml, 'Cargo.toml'));
  }
  return { violations, packageJsons: packageJsons.length, cargoMembers };
}

function main() {
  const here = path.dirname(fileURLToPath(import.meta.url));
  const root = path.resolve(here, '..', '..');
  const { violations, packageJsons, cargoMembers } = checkRepo(root, {
    excludedPaths: [path.join(here, 'fixtures')],
  });
  if (violations.length > 0) {
    console.error(`Found ${violations.length} dependency version(s) that are not exact:`);
    for (const v of violations) console.error(`  ${v.file} · ${v.dependency} · ${v.version}`);
    console.error('Pin them to an exact version (npm: "x.y.z", Cargo: "=x.y.z").');
    process.exit(1);
  }
  console.log(`exact-versions: OK (${packageJsons} package.json, ${cargoMembers} Cargo workspace members + workspace.dependencies)`);
}

if (process.argv[1] && import.meta.url === pathToFileURL(path.resolve(process.argv[1])).href) main();
