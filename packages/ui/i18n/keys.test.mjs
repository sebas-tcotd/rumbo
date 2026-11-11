import test from 'node:test';
import assert from 'node:assert/strict';
import { readdirSync, readFileSync } from 'node:fs';
import { join, relative } from 'node:path';
import { fileURLToPath } from 'node:url';

const pkg = fileURLToPath(new URL('../', import.meta.url));
const es = JSON.parse(readFileSync(new URL('./es.json', import.meta.url), 'utf8'));
/** `pantalla.elemento`: camelCase segments; error codes keep their UPPER_SNAKE name under `errors.`. */
const KEY = /^[a-z][a-zA-Z0-9]*(\.[a-z][a-zA-Z0-9]*)+$|^errors\.[A-Z][A-Z0-9_]*$/;
const NOTICE_CODES = ['SIM_NOT_READY', 'INVALID_PROFILE', 'EXPORT_FAILED', 'UNSUPPORTED_OPERATION'];

function flatten(node, prefix = '') {
  return Object.entries(node).flatMap(([k, v]) =>
    typeof v === 'string' ? [[prefix + k, v]] : flatten(v, prefix + k + '.'),
  );
}
const keys = new Map(flatten(es));

function sources(dir) {
  return readdirSync(dir, { withFileTypes: true }).flatMap((e) => {
    if (e.name === 'node_modules' || e.name === 'dist') return [];
    const path = join(dir, e.name);
    if (e.isDirectory()) return sources(path);
    return /\.(ts|tsx)$/.test(e.name) && !e.name.endsWith('.d.ts') ? [path] : [];
  });
}

test('every key in es.json follows pantalla.elemento in camelCase and has text', () => {
  assert.ok(keys.size > 0);
  for (const [key, text] of keys) {
    assert.match(key, KEY, key);
    assert.ok(text.trim().length > 0, key);
  }
});

test('every literal key used by the ui source exists in es.json', () => {
  // t('…') calls and the key literals returned by helpers such as idleKey().
  const literal = /\bt\(\s*'([^'$]+)'|'((?:lab|hud|errors|fatal)\.[A-Za-z0-9_.]+)'/g;
  const used = new Set();
  for (const file of sources(pkg)) {
    for (const m of readFileSync(file, 'utf8').matchAll(literal)) used.add([m[1] ?? m[2], relative(pkg, file)]);
  }
  assert.ok(used.size > 10, 'the scan found the HUD keys');
  for (const [key, file] of used) assert.ok(keys.has(key), `${file}: missing key ${key}`);
});

test('every notice code the store can send has a translation', () => {
  for (const code of NOTICE_CODES) assert.ok(keys.has('errors.' + code), code);
  const snapshot = readFileSync(new URL('../ui-snapshot.ts', import.meta.url), 'utf8');
  const declared = /UiNoticeCode =([^;]+);/.exec(snapshot)[1].match(/'([A-Z_]+)'/g).map((s) => s.slice(1, -1));
  assert.deepEqual(declared.sort(), [...NOTICE_CODES].sort());
});

test('the instance is created per mount, never the global i18next', () => {
  const create = readFileSync(new URL('./create-i18n.ts', import.meta.url), 'utf8');
  assert.match(create, /i18next\.createInstance\(\)/);
  assert.match(create, /initAsync: false/);
  for (const file of sources(pkg)) {
    const text = readFileSync(file, 'utf8');
    assert.doesNotMatch(text, /i18next\.(init|use|t)\(/, relative(pkg, file));
  }
});
