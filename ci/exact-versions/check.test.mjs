// Tests for the exact-version check: `node --test "ci/exact-versions/*.test.mjs"`.
import assert from 'node:assert/strict';
import { execFileSync } from 'node:child_process';
import path from 'node:path';
import { describe, it } from 'node:test';
import { fileURLToPath } from 'node:url';

import {
  checkCargoMetadata,
  checkPackageJson,
  checkPnpmCatalog,
  checkRepo,
  checkWorkspaceDependencies,
  findPackageJsons,
  isAllowedCargoDep,
  isExactCargoReq,
  isExactNpmSpec,
  parsePnpmCatalog,
  parseWorkspaceDependencies,
} from './check.mjs';

const here = path.dirname(fileURLToPath(import.meta.url));
const fixture = (name) => path.join(here, 'fixtures', name);
const noCargo = { packages: [] };

describe('isExactNpmSpec', () => {
  for (const spec of ['1.2.3', '0.187.0', '7.0.2-rc.1', '1.0.0-beta.2+build.5', 'workspace:*', 'catalog:', 'catalog:react19']) {
    it(`accepts ${spec}`, () => assert.equal(isExactNpmSpec(spec), true));
  }
  for (const spec of [
    '^1.0.0',
    '~1.3.0',
    '*',
    'latest',
    '>=1.0.0',
    '<2',
    '1.x',
    '1.2.x',
    '1',
    '1.2',
    '>=1.0.0 <2.0.0',
    '1.0.0 - 2.0.0',
    '1.0.0 || 2.0.0',
    'workspace:^',
    'npm:left-pad@^1.0.0',
    '',
    undefined,
  ]) {
    it(`rejects ${JSON.stringify(spec)}`, () => assert.equal(isExactNpmSpec(spec), false));
  }
});

describe('isExactCargoReq / isAllowedCargoDep', () => {
  it('accepts "=1.2.3"', () => assert.equal(isExactCargoReq('=1.2.3'), true));
  it('accepts "=0.36.1"', () => assert.equal(isExactCargoReq('=0.36.1'), true));
  for (const req of ['1', '0.36', '^0.36.1', '~0.36.1', '0.36.1', '*', '>=1.0.0', '=1.2', '=0.2, <0.3']) {
    it(`rejects ${JSON.stringify(req)}`, () => assert.equal(isExactCargoReq(req), false));
  }
  it('allows a path-only dependency (cargo metadata reports req "*")', () =>
    assert.equal(isAllowedCargoDep({ req: '*', path: '/repo/crates/sim-core' }), true));
  it('allows a path-only dependency with no req', () => assert.equal(isAllowedCargoDep({ path: '../sim-core' }), true));
  it('rejects a path dependency that also carries a range', () =>
    assert.equal(isAllowedCargoDep({ req: '^0.1', path: '../sim-core' }), false));
  it('rejects "*" without a path', () => assert.equal(isAllowedCargoDep({ req: '*' }), false));
});

describe('checkPackageJson', () => {
  it('reports ranges in dependencies, devDependencies and optionalDependencies', () => {
    const violations = checkPackageJson(
      {
        dependencies: { a: '^1.0.0', ok: '1.0.0' },
        devDependencies: { b: '~2.0.0' },
        optionalDependencies: { c: 'latest' },
      },
      'package.json',
    );
    assert.deepEqual(
      violations.map((v) => v.dependency),
      ['a (dependencies)', 'b (devDependencies)', 'c (optionalDependencies)'],
    );
    assert.equal(violations[0].version, '^1.0.0');
    assert.equal(violations[0].file, 'package.json');
  });
  it('ignores peerDependencies and engines', () => {
    assert.deepEqual(checkPackageJson({ peerDependencies: { react: '>=18' }, engines: { node: '>=24 <25' } }, 'x'), []);
  });
});

describe('checkCargoMetadata', () => {
  const metadata = {
    packages: [
      {
        name: 'sim-core',
        manifest_path: '/repo/crates/sim-core/Cargo.toml',
        dependencies: [
          { name: 'rapier3d', req: '=0.36.1', kind: null, path: undefined },
          { name: 'libm', req: '0.2', kind: null },
          { name: 'serde', req: '^1', kind: 'dev' },
          { name: 'sim-other', req: '*', kind: null, path: '/repo/crates/sim-other' },
        ],
      },
    ],
  };
  it('reports only the non-exact requirements', () => {
    const violations = checkCargoMetadata(metadata, '/repo');
    assert.deepEqual(
      violations.map((v) => `${v.dependency} ${v.version}`),
      ['libm (dependencies) 0.2', 'serde (dev-dependencies) ^1'],
    );
    assert.equal(violations[0].file, path.join('crates', 'sim-core', 'Cargo.toml'));
  });
});

describe('workspace.dependencies parser', () => {
  const toml = `
[workspace]
members = ["a"]

[workspace.dependencies]
# comment
rapier3d = { version = "=0.36.1", default-features = false, features = ["std"] }
wasm-bindgen = "=0.2.130"
libm = "0.2"   # trailing comment
sim-core = { path = "crates/sim-core" }
serde = { version = "1", features = ["derive"] }

[workspace.dependencies.blake3]
version = "^1.8"

[profile.release]
opt-level = 3
`;
  it('reads inline, string and sub-table entries, stopping at the next table', () => {
    assert.deepEqual(
      parseWorkspaceDependencies(toml).map((d) => `${d.name}=${d.req ?? ''}|${d.path ?? ''}`),
      [
        'rapier3d==0.36.1|',
        'wasm-bindgen==0.2.130|',
        'libm=0.2|',
        'sim-core=|crates/sim-core',
        'serde=1|',
        'blake3=^1.8|',
      ],
    );
  });
  it('reports the non-exact entries', () => {
    assert.deepEqual(
      checkWorkspaceDependencies(toml, 'Cargo.toml').map((v) => `${v.dependency} ${v.version}`),
      ['libm (workspace.dependencies) 0.2', 'serde (workspace.dependencies) 1', 'blake3 (workspace.dependencies) ^1.8'],
    );
  });
  it('handles CRLF line endings', () => {
    assert.equal(parseWorkspaceDependencies(toml.replace(/\n/g, '\r\n')).length, 6);
  });
  it('reads single-quoted keys and values', () => {
    const deps = parseWorkspaceDependencies("[workspace.dependencies]\n'foo' = '=1.2.3'\n'bar' = '^2'\n");
    assert.deepEqual(
      deps.map((d) => `${d.name}=${d.req}`),
      ['foo==1.2.3', 'bar=^2'],
    );
    assert.deepEqual(
      checkWorkspaceDependencies("[workspace.dependencies]\n'bar' = '^2'\n", 'Cargo.toml').map((v) => v.version),
      ['^2'],
    );
  });
  it('fails closed on lines it cannot parse (dotted keys, multi-line inline tables)', () => {
    const violations = checkWorkspaceDependencies(
      '[workspace.dependencies]\nfoo.version = "=1.0.0"\nbar = { version = "=1.0.0", features = [\n  "x",\n] }\n',
      'Cargo.toml',
    );
    const names = violations.map((v) => v.dependency);
    assert.ok(names.includes('foo.version = "=1.0.0" (workspace.dependencies)'));
    assert.ok(names.includes('"x", (workspace.dependencies)'));
    assert.ok(violations.every((v) => v.version.length > 0));
  });
});

describe('fixtures (intentional violations)', () => {
  it('fails on a caret range', () => {
    const { violations } = checkRepo(fixture('caret'), { cargoMetadata: noCargo });
    assert.equal(violations.length, 1);
    assert.equal(violations[0].version, '^1.0.0');
  });
  it('fails on a tilde range', () => {
    const { violations } = checkRepo(fixture('tilde'), { cargoMetadata: noCargo });
    assert.equal(violations.length, 1);
    assert.equal(violations[0].version, '~1.3.0');
  });
  it('passes on exact versions (peerDependencies and engines are not checked)', () => {
    assert.deepEqual(checkRepo(fixture('clean'), { cargoMetadata: noCargo }).violations, []);
  });
  it('passes on workspace:* and catalog:', () => {
    assert.deepEqual(checkRepo(fixture('workspace'), { cargoMetadata: noCargo }).violations, []);
  });
  it('excludes the fixtures dir when asked to', () => {
    const all = findPackageJsons(here);
    const filtered = findPackageJsons(here, [path.join(here, 'fixtures')]);
    // The folder's own package.json (the exact-versions workspace package) is not a fixture.
    assert.equal(all.length, 5);
    assert.deepEqual(filtered, [path.join(here, 'package.json')]);
  });
});

describe('pnpm catalog', () => {
  const clean = `packages:
  - apps/*

minimumReleaseAge: 10080 # minutes
catalog:
  '@types/react': 19.3.0 # scoped key
  "react": "19.3.1"
  three: 0.187.0

other:
  foo: ^1.0.0
`;
  it('passes on a clean catalog and ignores other top-level keys', () => {
    assert.deepEqual(checkPnpmCatalog(clean, 'pnpm-workspace.yaml'), []);
    assert.deepEqual(
      parsePnpmCatalog(clean).map((e) => [e.name, e.spec]),
      [['@types/react', '19.3.0'], ['react', '19.3.1'], ['three', '0.187.0']],
    );
  });
  for (const bad of ['^1.0.0', '~1.3.0', 'latest', '1.x', '>=1.0.0']) {
    it(`fails on ${bad} in the default catalog`, () => {
      const v = checkPnpmCatalog(`catalog:\n  foo: ${bad}\n`, 'w.yaml');
      assert.equal(v.length, 1);
      assert.equal(v[0].version, bad);
      assert.equal(v[0].dependency, 'foo (catalog)');
    });
  }
  it('checks named catalogs', () => {
    const yaml = 'catalogs:\n  legacy:\n    foo: 1.0.0\n    bar: ^2.0.0\n  next:\n    baz: 3.0.0\n';
    const v = checkPnpmCatalog(yaml, 'w.yaml');
    assert.equal(v.length, 1);
    assert.equal(v[0].dependency, 'bar (catalog:legacy)');
  });
  it('fails closed on lines it cannot read', () => {
    const bad = [
      'catalog:\n  foo:\n    version: 1.0.0\n',
      'catalog:\n  - foo\n',
      'catalog: { foo: 1.0.0 }\n',
      'catalogs:\n  foo: 1.0.0\n',
    ];
    for (const yaml of bad) {
      const v = checkPnpmCatalog(yaml, 'w.yaml');
      assert.ok(v.length >= 1 && v.every((x) => /unparsed/.test(x.version)), yaml);
    }
  });
  it('accepts an empty catalog', () => {
    assert.deepEqual(checkPnpmCatalog('catalog: {}\npackages: []\n', 'w.yaml'), []);
  });
  it('checkRepo reads pnpm-workspace.yaml when present', () => {
    const { violations } = checkRepo(fixture('catalog-caret'), { cargoMetadata: noCargo });
    assert.equal(violations.length, 1);
    assert.equal(violations[0].file, 'pnpm-workspace.yaml');
  });
  it('still accepts "catalog:" in a package.json', () => {
    assert.deepEqual(checkPackageJson({ dependencies: { react: 'catalog:', vite: 'catalog:legacy' } }, 'p.json'), []);
  });
});

describe('CLI', () => {
  it('exits 0 on the real repo (fixtures excluded)', () => {
    const out = execFileSync(process.execPath, [path.join(here, 'check.mjs')], { encoding: 'utf8' });
    assert.match(out, /exact-versions: OK/);
  });
});
