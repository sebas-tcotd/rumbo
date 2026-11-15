// Story 0.9 · P1 golden rule: a commit that adds, deletes, edits or renames a golden replay must have the
// subject prefix `physics:` and raise PHYSICS_VERSION in that same commit.
//
// Every commit introduced by the push or PR is evaluated on its own against its parent(s), so an edit
// reverted later is still subject to P1 and a bump made in another commit does not authorize it.
// - Ordinary commit: golden entries that differ from the parent.
// - Merge commit: only the changes the resolution itself introduces (entries that differ from EVERY
//   parent). A golden inherited unchanged from one parent is covered by the commit that introduced it,
//   which is in the range too when it is new.
// - Root commit: every golden it contains counts as added (there is no previous version to raise).
//
// Ranges (GitHub event payload):
// - pull_request: commits reachable from `pull_request.head.sha` and not from `pull_request.base.sha`
//   (`git rev-list head ^base`), forks included: the checkout of the merge ref brings the head history.
// - push: `git rev-list after ^before`. It covers fast-forwards and forced pushes alike (a force push is
//   checked on the commits the new tip introduces relative to the old one). If `before` is outside the
//   downloaded history (force push over a rewritten branch), it is fetched by SHA from origin; if it still
//   cannot be resolved, the check fails explicitly instead of guessing.
// - push of a new branch (`before` = 000…0): commits reachable from `after` and from no other remote
//   branch or tag already fetched. The first push of the repository checks its whole history.
// - push deleting a branch (`after` = 000…0 or `deleted`): nothing is introduced; nothing to check.
// The workflow checks out with `fetch-depth: 0`; a shallow repository is an explicit error.
//
// E0 signal: PHYSICS_VERSION in crates/track-format/src/version.rs, compared numerically (MAJOR.MINOR.PATCH).
// A downgrade, an equal or cosmetic change, a missing or non-numeric constant, or a bump in another commit do
// not authorize the change. Story 1.1 adds the canonical rules-asset hash as a second valid signal: a commit
// will also be authorized when that versioned asset's canonical hash changes in the same commit. No proxy
// signal is accepted meanwhile (a hash written in the message, any JSON change, the track or profile hash).
//
// Usage (CI): node ci/golden-rule/check.mjs            (reads GITHUB_EVENT_NAME and GITHUB_EVENT_PATH)
// Usage (local): node ci/golden-rule/check.mjs --base <sha> --head <sha>
//                node ci/golden-rule/check.mjs --before <sha> --after <sha> [--ref refs/heads/x]
import { execFileSync } from 'node:child_process';
import { readFileSync } from 'node:fs';
import { resolve } from 'node:path';
import { fileURLToPath } from 'node:url';

export const GOLDEN_DIR = 'crates/sim-core/tests/golden/';
export const VERSION_FILE = 'crates/track-format/src/version.rs';
export const SUBJECT = /^physics:/;
const ZERO = /^0+$/;

function git(cwd, args) {
  return execFileSync('git', args, { cwd, encoding: 'utf8', stdio: ['ignore', 'pipe', 'pipe'], maxBuffer: 64 * 1024 * 1024 });
}

function tryGit(cwd, args) {
  try {
    return git(cwd, args);
  } catch {
    return null;
  }
}

export function hasCommit(cwd, sha) {
  return tryGit(cwd, ['cat-file', '-e', `${sha}^{commit}`]) !== null;
}

/** Makes sure `sha` is available, fetching it by SHA from origin if needed. Throws an explicit error otherwise. */
export function ensureCommit(cwd, sha, label) {
  if (!sha || !/^[0-9a-f]{7,64}$/i.test(sha)) throw new Error(`rango no resoluble: ${label} inválido (${sha})`);
  if (hasCommit(cwd, sha)) return;
  if (tryGit(cwd, ['remote', 'get-url', 'origin']) !== null) tryGit(cwd, ['fetch', '--no-tags', '--quiet', 'origin', sha]);
  if (!hasCommit(cwd, sha)) throw new Error(`rango no resoluble: ${label} ${sha} no está en el historial descargado ni se pudo traer de origin`);
}

/** Normalizes a GitHub event payload into `{ kind, ... }`. */
export function fromGithubEvent(name, payload) {
  if (name === 'pull_request' || name === 'pull_request_target') {
    return { kind: 'pr', base: payload?.pull_request?.base?.sha, head: payload?.pull_request?.head?.sha };
  }
  if (name === 'push') {
    return { kind: 'push', before: payload?.before, after: payload?.after, ref: payload?.ref, deleted: payload?.deleted === true };
  }
  return { kind: 'none', name };
}

/** Commits introduced by the event, oldest first. Throws on a range that cannot be resolved. */
export function resolveRange(cwd, event) {
  if (event.kind === 'none') return { commits: [], note: `evento ${event.name}: sin commits que revisar` };
  if (tryGit(cwd, ['rev-parse', '--is-shallow-repository'])?.trim() === 'true') {
    throw new Error('rango no resoluble: el repositorio es superficial (usa actions/checkout con fetch-depth: 0)');
  }
  const list = (args) => git(cwd, ['rev-list', '--reverse', '--topo-order', ...args]).split('\n').filter(Boolean);
  if (event.kind === 'pr') {
    ensureCommit(cwd, event.base, 'base del PR');
    ensureCommit(cwd, event.head, 'head del PR');
    return { commits: list([event.head, `^${event.base}`]), note: `PR ${event.base.slice(0, 7)}..${event.head.slice(0, 7)}` };
  }
  if (event.deleted || ZERO.test(event.after ?? '')) return { commits: [], note: 'push que elimina una rama: nada introducido' };
  ensureCommit(cwd, event.after, 'after del push');
  if (ZERO.test(event.before ?? '')) {
    // New branch or tag: exclude everything already known through the other remote refs and tags.
    const own = event.ref?.startsWith('refs/heads/') ? `refs/remotes/origin/${event.ref.slice('refs/heads/'.length)}` : null;
    const others = git(cwd, ['for-each-ref', '--format=%(refname)', 'refs/remotes/', 'refs/tags/'])
      .split('\n')
      .filter((r) => r && r !== own && r !== event.ref && !r.endsWith('/HEAD'));
    return { commits: list([event.after, ...others.map((r) => `^${r}`)]), note: `rama nueva ${event.ref ?? ''}: commits ausentes de las demás ramas` };
  }
  ensureCommit(cwd, event.before, 'before del push');
  return { commits: list([event.after, `^${event.before}`]), note: `push ${event.before.slice(0, 7)}..${event.after.slice(0, 7)}` };
}

/** Golden entries of a tree: path → Git mode, type and object ID. */
export function goldenTree(cwd, rev) {
  const out = git(cwd, ['ls-tree', '-r', '-z', rev, '--', GOLDEN_DIR]);
  const map = new Map();
  for (const entry of out.split('\0').filter(Boolean)) {
    const tab = entry.indexOf('\t');
    map.set(entry.slice(tab + 1), entry.slice(0, tab));
  }
  return map;
}

/** PHYSICS_VERSION at `rev`, or null when the file or the constant is missing. */
export function readVersion(cwd, rev) {
  const src = tryGit(cwd, ['show', `${rev}:${VERSION_FILE}`]);
  if (src === null) return null;

  // Read Rust tokens so comments and string literals cannot be mistaken for declarations.
  const token = /\/\/[^\r\n]*|\/\*|[bc]?r(#*)"[\s\S]*?"\1|[bc]?"(?:\\[\s\S]|[^"\\])*"|'(?:\\.|[^'\\])'|[A-Za-z_]\w*|[^\s]/g;
  const block = /\/\*|\*\//g;
  const tokens = [];
  let nesting = 0;
  for (let match; (match = token.exec(src));) {
    const value = match[0];
    if (value.startsWith('//')) continue;
    if (value === '/*') {
      block.lastIndex = token.lastIndex;
      let depth = 1;
      for (let delimiter; depth && (delimiter = block.exec(src));) depth += delimiter[0] === '/*' ? 1 : -1;
      if (depth) return null;
      token.lastIndex = block.lastIndex;
      continue;
    }
    if (nesting === 0) tokens.push(value);
    if (value === '{' || value === '(' || value === '[') nesting++;
    if (value === '}' || value === ')' || value === ']') nesting--;
  }

  const declaration = ['pub', 'const', 'PHYSICS_VERSION', ':', '&', 'str', '='];
  const at = tokens.findIndex((_, i) => declaration.every((part, j) => tokens[i + j] === part));
  if (at < 0 || tokens[at + declaration.length + 1] !== ';') return null;
  const literal = tokens[at + declaration.length];
  return /^"([^"\\]*)"$/.exec(literal)?.[1] ?? /^r(#*)"([\s\S]*)"\1$/.exec(literal)?.[2] ?? null;
}

export function parseVersion(v) {
  const m = /^(\d+)\.(\d+)\.(\d+)$/.exec(v ?? '');
  return m ? m.slice(1).map(Number) : null;
}

/** -1, 0 or 1 comparing two parsed versions numerically. */
export function compareVersions(a, b) {
  for (let i = 0; i < 3; i++) if (a[i] !== b[i]) return a[i] < b[i] ? -1 : 1;
  return 0;
}

/** Golden changes a commit introduces relative to its parents (see the header for merges and roots). */
export function goldenChanges(cwd, sha, parents) {
  const cur = goldenTree(cwd, sha);
  const before = parents.length ? parents.map((p) => goldenTree(cwd, p)) : [new Map()];
  const paths = new Set([...cur.keys(), ...before.flatMap((m) => [...m.keys()])]);
  const changed = [...paths].filter((p) => before.every((m) => m.get(p) !== cur.get(p))).sort();
  const added = changed.filter((p) => cur.has(p) && !before.some((m) => m.has(p)));
  const deleted = changed.filter((p) => !cur.has(p));
  const modified = changed.filter((p) => !added.includes(p) && !deleted.includes(p));
  const changes = modified.map((path) => ({ kind: 'M', path }));
  const usedAdds = new Set();
  const objectId = (entry) => entry?.split(' ')[2];
  for (const from of deleted) {
    const blobs = new Set(before.map((m) => objectId(m.get(from))).filter(Boolean));
    const to = added.find((a) => !usedAdds.has(a) && blobs.has(objectId(cur.get(a))));
    if (to) {
      usedAdds.add(to);
      changes.push({ kind: 'R', from, path: to });
    } else changes.push({ kind: 'D', path: from });
  }
  for (const a of added) if (!usedAdds.has(a)) changes.push({ kind: 'A', path: a });
  return changes;
}

export function describeChange(c) {
  const what = { A: 'alta', D: 'baja', M: 'edición', R: 'renombre' }[c.kind];
  return c.kind === 'R' ? `${what} ${c.from} → ${c.path}` : `${what} ${c.path}`;
}

/** Evaluates one commit. Returns `{ sha, subject, changes, violations }`. */
export function checkCommit(cwd, sha) {
  const [, ...parents] = git(cwd, ['rev-list', '--parents', '-n', '1', sha]).trim().split(' ');
  const changes = goldenChanges(cwd, sha, parents);
  const subject = git(cwd, ['log', '-1', '--format=%s', sha]).trim();
  const violations = [];
  if (!changes.length) return { sha, subject, changes, violations };
  if (!SUBJECT.test(subject)) violations.push('el asunto no empieza con «physics:»');
  const next = readVersion(cwd, sha);
  const parsedNext = parseVersion(next);
  if (next === null) violations.push(`PHYSICS_VERSION ausente en ${VERSION_FILE}`);
  else if (!parsedNext) violations.push(`PHYSICS_VERSION no numérica: «${next}»`);
  if (!parents.length) violations.push('commit raíz: no hay versión anterior que subir');
  for (const p of parents) {
    const prev = readVersion(cwd, p);
    const parsedPrev = parseVersion(prev);
    if (prev === null) violations.push(`PHYSICS_VERSION ausente en el padre ${p.slice(0, 7)}`);
    else if (!parsedPrev) violations.push(`PHYSICS_VERSION no numérica en el padre ${p.slice(0, 7)}: «${prev}»`);
    else if (parsedNext && compareVersions(parsedNext, parsedPrev) <= 0) {
      violations.push(`PHYSICS_VERSION no sube en este commit (${prev} → ${next}${parents.length > 1 ? ` respecto de ${p.slice(0, 7)}` : ''})`);
    }
  }
  return { sha, subject, changes, violations };
}

/** Full check for an event. Throws only when the range cannot be resolved. */
export function check(cwd, event) {
  const { commits, note } = resolveRange(cwd, event);
  const results = commits.map((sha) => checkCommit(cwd, sha));
  return { ok: results.every((r) => r.violations.length === 0), note, results };
}

export function report({ ok, note, results }) {
  const touched = results.filter((r) => r.changes.length);
  const lines = [`P1 · regla de dorados · ${note} · ${results.length} commits, ${touched.length} tocan dorados`];
  for (const r of touched) {
    const head = `${r.violations.length ? '❌' : '✅'} ${r.sha.slice(0, 12)} «${r.subject}»`;
    lines.push(head, ...r.changes.map((c) => `     ${describeChange(c)}`), ...r.violations.map((v) => `     P1 incumplida: ${v}`));
  }
  lines.push(ok ? 'P1 cumplida.' : 'P1 incumplida: un commit que toca un dorado necesita el prefijo «physics:» y subir PHYSICS_VERSION en ese mismo commit.');
  return lines.join('\n');
}

if (process.argv[1] && resolve(process.argv[1]) === fileURLToPath(import.meta.url)) {
  const arg = (k) => {
    const i = process.argv.indexOf(k);
    return i > 0 ? process.argv[i + 1] : undefined;
  };
  // Local convenience: accept any revision (HEAD, origin/main) and pass its SHA on. GitHub payloads are SHAs.
  const sha = (k) => {
    const v = arg(k);
    return v && !/^[0-9a-f]{40}$/i.test(v) ? (tryGit(process.cwd(), ['rev-parse', '--verify', '--quiet', `${v}^{commit}`])?.trim() ?? v) : v;
  };
  let event;
  if (arg('--base') || arg('--head')) event = { kind: 'pr', base: sha('--base'), head: sha('--head') };
  else if (arg('--before') || arg('--after')) event = { kind: 'push', before: sha('--before'), after: sha('--after'), ref: arg('--ref') };
  else if (process.env.GITHUB_EVENT_PATH) {
    event = fromGithubEvent(process.env.GITHUB_EVENT_NAME, JSON.parse(readFileSync(process.env.GITHUB_EVENT_PATH, 'utf8')));
  } else {
    console.error('uso: check.mjs --base <sha> --head <sha> | --before <sha> --after <sha> [--ref <ref>] (o GITHUB_EVENT_PATH)');
    process.exit(2);
  }
  try {
    const result = check(process.cwd(), event);
    console.log(report(result));
    process.exit(result.ok ? 0 : 1);
  } catch (e) {
    console.error(`P1 · regla de dorados: ${e.message}`);
    process.exit(1);
  }
}
