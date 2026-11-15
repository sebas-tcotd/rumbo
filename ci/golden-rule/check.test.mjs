// Story 0.9: P1 on small temporary Git histories (positive and negative cases).
import assert from 'node:assert/strict';
import { execFileSync } from 'node:child_process';
import { mkdirSync, mkdtempSync, renameSync, rmSync, unlinkSync, writeFileSync } from 'node:fs';
import { tmpdir } from 'node:os';
import { dirname, join } from 'node:path';
import test from 'node:test';
import { check, compareVersions, fromGithubEvent, GOLDEN_DIR, parseVersion, report, VERSION_FILE } from './check.mjs';

const ZERO = '0'.repeat(40);

/** A throwaway repository with a version file and two goldens on `main`. */
function repo() {
  const dir = mkdtempSync(join(tmpdir(), 'rumbo-p1-'));
  const git = (...args) => execFileSync('git', args, { cwd: dir, encoding: 'utf8', stdio: ['ignore', 'pipe', 'pipe'] }).trim();
  git('init', '-q', '-b', 'main');
  git('config', 'user.email', 'p1@example.invalid');
  git('config', 'user.name', 'P1');
  git('config', 'commit.gpgsign', 'false');
  git('config', 'core.autocrlf', 'false');
  const write = (path, text) => {
    mkdirSync(dirname(join(dir, path)), { recursive: true });
    writeFileSync(join(dir, path), text);
  };
  const version = (v, extra = '') => write(VERSION_FILE, `${extra}pub const PHYSICS_VERSION: &str = "${v}";\npub const TICK_HZ: u32 = 120;\n`);
  const golden = (name, text) => write(`${GOLDEN_DIR}${name}`, text);
  const commit = (msg) => {
    git('add', '-A');
    git('commit', '-q', '--allow-empty', '-m', msg);
    return git('rev-parse', 'HEAD');
  };
  version('0.5.0');
  golden('a.json', '{"a":1}');
  golden('b.json', '{"b":1}');
  write('README.md', 'rumbo');
  const base = commit('chore: base');
  return {
    dir, git, write, version, golden, commit, base,
    rm: (path) => unlinkSync(join(dir, path)),
    mv: (from, to) => renameSync(join(dir, from), join(dir, to)),
    pr: (head, b = base) => check(dir, { kind: 'pr', base: b, head }),
    push: (before, after, ref = 'refs/heads/main') => check(dir, { kind: 'push', before, after, ref }),
    done: () => rmSync(dir, { recursive: true, force: true }),
  };
}

const violations = (r) => r.results.flatMap((x) => x.violations.map((v) => `${x.subject} :: ${v}`));

test('a physics: commit that edits a golden and raises PHYSICS_VERSION passes', () => {
  const r = repo();
  try {
    r.golden('a.json', '{"a":2}');
    r.version('0.6.0');
    const head = r.commit('physics: retune grip');
    const res = r.pr(head);
    assert.equal(res.ok, true, violations(res).join('\n'));
    assert.deepEqual(res.results[0].changes, [{ kind: 'M', path: `${GOLDEN_DIR}a.json` }]);
  } finally {
    r.done();
  }
});

test('the version comparison is numeric (0.10.0 > 0.9.9)', () => {
  assert.equal(compareVersions(parseVersion('0.10.0'), parseVersion('0.9.9')), 1);
  assert.equal(parseVersion('0.5'), null);
  assert.equal(parseVersion('v0.5.0'), null);
  const r = repo();
  try {
    r.version('0.9.9');
    const mid = r.commit('chore: 0.9.9');
    r.golden('a.json', '{"a":3}');
    r.version('0.10.0');
    assert.equal(r.pr(r.commit('physics: big change'), mid).ok, true);
  } finally {
    r.done();
  }
});

test('a missing prefix fails and names commit, file and condition', () => {
  const r = repo();
  try {
    r.golden('a.json', '{"a":2}');
    r.version('0.6.0');
    const head = r.commit('fix: retune grip');
    const res = r.pr(head);
    assert.equal(res.ok, false);
    const text = report(res);
    assert.match(text, new RegExp(head.slice(0, 12)));
    assert.match(text, /edición crates\/sim-core\/tests\/golden\/a\.json/);
    assert.match(text, /el asunto no empieza con «physics:»/);
  } finally {
    r.done();
  }
});

test('an equal, lower or cosmetic version fails', () => {
  for (const [v, extra] of [['0.5.0', ''], ['0.4.9', ''], ['0.5.0', '// cosmetic\n']]) {
    const r = repo();
    try {
      r.golden('a.json', '{"a":2}');
      r.version(v, extra);
      const res = r.pr(r.commit('physics: no real bump'));
      assert.equal(res.ok, false, `${v} ${extra}`);
      assert.match(violations(res).join('\n'), /PHYSICS_VERSION no sube en este commit/);
    } finally {
      r.done();
    }
  }
});

test('a missing or non-numeric constant fails', () => {
  const r = repo();
  try {
    r.golden('a.json', '{"a":2}');
    r.write(VERSION_FILE, 'pub const TICK_HZ: u32 = 120;\n');
    assert.match(violations(r.pr(r.commit('physics: lost constant'))).join('\n'), /PHYSICS_VERSION ausente/);
    r.golden('a.json', '{"a":3}');
    r.version('0.6.0-beta');
    assert.match(violations(r.pr(r.commit('physics: odd version'), r.git('rev-parse', 'HEAD~1'))).join('\n'), /no numérica/);
  } finally {
    r.done();
  }
});

test('only the live PHYSICS_VERSION declaration supplies the bump', () => {
  const r = repo();
  try {
    r.golden('a.json', '{"a":2}');
    const comments = '/// pub const PHYSICS_VERSION: &str = "9.0.0";\n' +
      '/* historical values /* nested note */\npub const PHYSICS_VERSION: &str = "8.0.0";\n*/\n';
    r.version('0.5.0', comments);
    assert.match(violations(r.pr(r.commit('physics: unchanged version'))).join('\n'), /PHYSICS_VERSION no sube/);

    const base = r.git('rev-parse', 'HEAD');
    r.golden('a.json', '{"a":3}');
    r.write(VERSION_FILE, `${comments}pub const PHYSICS_VERSION: /* version */ &str = r#"0.6.0"#;\n`);
    assert.equal(r.pr(r.commit('physics: bump the live version'), base).ok, true);
  } finally {
    r.done();
  }
});

test('a mode-only change to a golden still requires authorization', () => {
  const r = repo();
  try {
    const path = `${GOLDEN_DIR}a.json`;
    r.git('update-index', '--chmod=+x', path);
    r.git('commit', '-q', '-m', 'chore: change golden mode');
    const result = r.pr(r.git('rev-parse', 'HEAD'));
    assert.equal(result.ok, false);
    assert.deepEqual(result.results[0].changes, [{ kind: 'M', path }]);
  } finally {
    r.done();
  }
});

test('a bump made in another commit does not authorize the golden change', () => {
  const r = repo();
  try {
    r.version('0.6.0');
    r.commit('physics: bump first');
    r.golden('a.json', '{"a":2}');
    const head = r.commit('physics: then touch the golden');
    const res = r.pr(head);
    assert.equal(res.ok, false);
    assert.equal(res.results.find((x) => x.sha === head).violations.length, 1);
  } finally {
    r.done();
  }
});

test('an edit reverted in a later commit is still subject to P1', () => {
  const r = repo();
  try {
    r.golden('a.json', '{"a":2}');
    r.commit('test: tweak golden');
    r.golden('a.json', '{"a":1}');
    const head = r.commit('revert: tweak golden');
    assert.equal(r.git('diff', r.base, head, '--', GOLDEN_DIR), '', 'net diff is empty');
    const res = r.pr(head);
    assert.equal(res.ok, false);
    assert.equal(res.results.filter((x) => x.violations.length).length, 2);
  } finally {
    r.done();
  }
});

test('an unauthorized addition, deletion or rename fails and a rename names origin and destination', () => {
  const r = repo();
  try {
    r.golden('c.json', '{"c":1}');
    const add = r.commit('test: add golden');
    r.rm(`${GOLDEN_DIR}b.json`);
    const del = r.commit('test: drop golden');
    r.mv(`${GOLDEN_DIR}a.json`, `${GOLDEN_DIR}z.json`);
    const ren = r.commit('test: rename golden');
    r.mv(`${GOLDEN_DIR}z.json`, 'moved-out.json');
    const out = r.commit('test: move golden out');
    const res = r.pr(out);
    assert.equal(res.ok, false);
    const by = (sha) => res.results.find((x) => x.sha === sha);
    assert.deepEqual(by(add).changes, [{ kind: 'A', path: `${GOLDEN_DIR}c.json` }]);
    assert.deepEqual(by(del).changes, [{ kind: 'D', path: `${GOLDEN_DIR}b.json` }]);
    assert.deepEqual(by(ren).changes, [{ kind: 'R', from: `${GOLDEN_DIR}a.json`, path: `${GOLDEN_DIR}z.json` }]);
    assert.deepEqual(by(out).changes, [{ kind: 'D', path: `${GOLDEN_DIR}z.json` }]);
    for (const sha of [add, del, ren, out]) assert.ok(by(sha).violations.length > 0);
    assert.match(report(res), /renombre crates\/sim-core\/tests\/golden\/a\.json → crates\/sim-core\/tests\/golden\/z\.json/);
  } finally {
    r.done();
  }
});

test('a PR without golden changes passes', () => {
  const r = repo();
  try {
    r.write('README.md', 'rumbo 2');
    r.version('0.4.0'); // version changes alone are not P1's business
    const res = r.pr(r.commit('docs: readme'));
    assert.equal(res.ok, true);
    assert.equal(res.results.length, 1);
  } finally {
    r.done();
  }
});

test('a merge that inherits a valid physics change passes', () => {
  const r = repo();
  try {
    r.git('checkout', '-q', '-b', 'feature');
    r.golden('a.json', '{"a":2}');
    r.version('0.6.0');
    r.commit('physics: retune');
    r.git('checkout', '-q', 'main');
    r.write('README.md', 'main moved');
    r.commit('docs: main moved');
    r.git('merge', '-q', '--no-ff', '-m', 'Merge feature', 'feature');
    const merge = r.git('rev-parse', 'HEAD');
    const res = r.pr(merge);
    assert.equal(res.ok, true, violations(res).join('\n'));
    assert.deepEqual(res.results.find((x) => x.sha === merge).changes, []);
  } finally {
    r.done();
  }
});

test('a merge whose resolution changes a golden without authorization fails', () => {
  const r = repo();
  try {
    r.git('checkout', '-q', '-b', 'feature');
    r.write('README.md', 'feature');
    r.commit('docs: feature');
    r.git('checkout', '-q', 'main');
    r.write('NOTES.md', 'main');
    r.commit('docs: main');
    r.git('merge', '-q', '--no-ff', '--no-commit', 'feature');
    r.golden('b.json', '{"b":"evil"}');
    const merge = r.commit('Merge feature');
    const res = r.pr(merge);
    assert.equal(res.ok, false);
    const m = res.results.find((x) => x.sha === merge);
    assert.deepEqual(m.changes, [{ kind: 'M', path: `${GOLDEN_DIR}b.json` }]);
    assert.ok(m.violations.some((v) => /physics:/.test(v)));
  } finally {
    r.done();
  }
});

test('the first push of a branch checks only the commits absent from the other remote branches', () => {
  const r = repo();
  try {
    r.git('update-ref', 'refs/remotes/origin/main', r.base);
    r.git('checkout', '-q', '-b', 'feature');
    r.golden('a.json', '{"a":2}');
    const bad = r.commit('test: unauthorized');
    r.write('README.md', 'x');
    const after = r.commit('docs: x');
    r.git('update-ref', 'refs/remotes/origin/feature', after); // what actions/checkout would have
    const res = r.push(ZERO, after, 'refs/heads/feature');
    assert.equal(res.results.length, 2);
    assert.equal(res.ok, false);
    assert.ok(res.results.find((x) => x.sha === bad).violations.length);
    // The base commit (already on origin/main) is not re-checked.
    assert.ok(!res.results.some((x) => x.sha === r.base));
  } finally {
    r.done();
  }
});

test('a normal push checks every commit between before and after', () => {
  const r = repo();
  try {
    r.write('README.md', 'one');
    const one = r.commit('docs: one');
    r.golden('a.json', '{"a":2}');
    r.version('0.6.0');
    const two = r.commit('physics: two');
    const res = r.push(r.base, two);
    assert.equal(res.ok, true);
    assert.deepEqual(res.results.map((x) => x.sha), [one, two]);
  } finally {
    r.done();
  }
});

test('a forced push checks the commits the new tip introduces over the old one', () => {
  const r = repo();
  try {
    r.write('README.md', 'old');
    const old = r.commit('docs: old tip');
    r.git('reset', '-q', '--hard', r.base);
    r.golden('b.json', '{"b":2}');
    const rewritten = r.commit('test: rewritten history touches a golden');
    const res = r.push(old, rewritten);
    assert.equal(res.ok, false);
    assert.deepEqual(res.results.map((x) => x.sha), [rewritten]);
  } finally {
    r.done();
  }
});

test('an incomplete range is an explicit error', () => {
  const r = repo();
  try {
    const missing = 'deadbeef'.repeat(5);
    assert.throws(() => r.push(missing, r.base), /rango no resoluble: before del push/);
    assert.throws(() => r.pr(r.base, missing), /rango no resoluble: base del PR/);
    assert.throws(() => r.pr(undefined), /rango no resoluble: head del PR inválido/);
  } finally {
    r.done();
  }
});

test('a push deleting a branch has nothing to check', () => {
  const r = repo();
  try {
    const res = check(r.dir, { kind: 'push', before: r.base, after: ZERO, ref: 'refs/heads/x', deleted: true });
    assert.equal(res.ok, true);
    assert.equal(res.results.length, 0);
  } finally {
    r.done();
  }
});

test('GitHub payloads are normalized', () => {
  assert.deepEqual(fromGithubEvent('pull_request', { pull_request: { base: { sha: 'b' }, head: { sha: 'h' } } }), { kind: 'pr', base: 'b', head: 'h' });
  assert.deepEqual(fromGithubEvent('push', { before: 'x', after: 'y', ref: 'refs/heads/main', deleted: false }), {
    kind: 'push', before: 'x', after: 'y', ref: 'refs/heads/main', deleted: false,
  });
  assert.equal(fromGithubEvent('workflow_dispatch', {}).kind, 'none');
});
