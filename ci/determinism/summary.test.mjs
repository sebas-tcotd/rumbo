// Story 0.9: the matrix summary fails on any missing, unexpected, duplicate, incomplete or failed result,
// and on a divergence in an intermediate tick even when the final hash matches. Uses the real fixtures and
// native reference (run `pnpm --filter rumbo-determinism build` first) plus temporary result files.
import assert from 'node:assert/strict';
import { spawnSync } from 'node:child_process';
import { mkdirSync, mkdtempSync, readFileSync, rmSync, writeFileSync } from 'node:fs';
import { tmpdir } from 'node:os';
import { dirname, join, resolve } from 'node:path';
import test from 'node:test';
import { fileURLToPath } from 'node:url';
import { listCases, REQUIRED_CASES } from './cases.mjs';
import { checkFixture, evaluateMatrix, firstDivergence, HEX_PER_TICK, REQUIRED_ENVS } from './evaluate.mjs';
import { summarize } from './summary.mjs';

const here = dirname(fileURLToPath(import.meta.url));
const root = resolve(here, '../..');
const referencePath = join(here, 'reference/native.json');
const reference = JSON.parse(readFileSync(referencePath, 'utf8'));
const fixtures = listCases(root);

/** A passing result file for `browser`, built from the native reference. */
function env(browser, mutate = (r) => r) {
  const results = reference.cases.map((c) => {
    const f = fixtures.find((x) => x.name === c.name);
    return mutate({ case: c.name, ticks: c.ticks, trace: c.trace, report: { ok: true, actualFinal: f.expected, message: 'ok' }, ms: 1 });
  });
  return { browser, userAgent: `test ${browser}`, physics: reference.physicsVersion, results: results.filter(Boolean) };
}
const full = () => REQUIRED_ENVS.map((b) => env(b));
const matrix = (envs, fx = fixtures, ref = reference) => evaluateMatrix(envs, fx, ref);
const flip = (hex) => (hex[0] === '0' ? '1' : '0') + hex.slice(1);

test('the eight goldens and the six environments are the required set', () => {
  assert.deepEqual(fixtures.map((f) => f.name), [...REQUIRED_CASES]);
  assert.deepEqual(reference.cases.map((c) => c.name), [...REQUIRED_CASES]);
  assert.deepEqual([...REQUIRED_ENVS], ['node', 'chromium', 'edge', 'firefox', 'webkit', 'safari']);
});

test('a complete matrix passes', () => {
  const m = matrix(full());
  assert.equal(m.ok, true, m.problems.join('\n'));
  assert.equal(m.envs.length, 6);
  assert.equal(m.cases.length, 8);
});

test('a missing environment fails', () => {
  const m = matrix(full().filter((e) => e.browser !== 'safari'));
  assert.equal(m.ok, false);
  assert.ok(m.problems.includes('falta el entorno safari'));
});

test('a duplicated environment fails', () => {
  const m = matrix([...full(), env('edge')]);
  assert.equal(m.ok, false);
  assert.ok(m.problems.includes('entorno duplicado: edge'));
});

test('an unexpected environment fails', () => {
  const m = matrix([...full(), env('opera')]);
  assert.equal(m.ok, false);
  assert.ok(m.problems.includes('entorno inesperado: opera'));
});

test('an environment-level error fails', () => {
  const envs = full();
  envs[2].error = 'boom';
  assert.equal(matrix(envs).ok, false);
});

test('a failed legacy verification fails', () => {
  const envs = full();
  envs[1] = env('chromium', (r) => (r.case === 'test-loop.scripted' ? { ...r, report: { ok: false, message: 'divergencia' } } : r));
  const m = matrix(envs);
  assert.equal(m.ok, false);
  assert.equal(m.cells['test-loop.scripted'].chromium.ok, false);
});

test('a result missing in one environment fails', () => {
  const envs = full();
  envs[3] = env('firefox', (r) => (r.case === 'san-borja-las-artes.autopilot' ? null : r));
  const m = matrix(envs);
  assert.equal(m.ok, false);
  assert.deepEqual(m.cells['san-borja-las-artes.autopilot'].firefox.problems, ['falta el resultado']);
});

test('a duplicated case in one environment fails', () => {
  const envs = full();
  envs[0].results.push({ ...envs[0].results[0] });
  const m = matrix(envs);
  assert.equal(m.ok, false);
  assert.ok(m.problems.some((p) => p.startsWith('node: caso duplicado')));
});

test('an unexpected case in one environment fails', () => {
  const envs = full();
  envs[0].results.push({ ...envs[0].results[0], case: 'otra-pista.autopilot' });
  assert.equal(matrix(envs).ok, false);
});

test('the same replay omitted everywhere (fixture, reference and every runner) still fails', () => {
  const gone = 'costa-verde-sprint.autopilot';
  const envs = REQUIRED_ENVS.map((b) => env(b, (r) => (r.case === gone ? null : r)));
  const m = matrix(envs, fixtures.filter((f) => f.name !== gone), { ...reference, cases: reference.cases.filter((c) => c.name !== gone) });
  assert.equal(m.ok, false);
  assert.ok(m.cases.includes(gone));
  for (const e of REQUIRED_ENVS) assert.equal(m.cells[gone][e].ok, false);
});

test('a single runner still needs all eight unique cases', () => {
  const gone = reference.cases[0].name;
  const run = env('node', (r) => (r.case === gone ? null : r));
  run.results.push(run.results[0]);
  const m = evaluateMatrix([run], fixtures.filter((f) => f.name !== gone),
    { ...reference, cases: reference.cases.slice(1) }, { requiredEnvs: ['node'] });
  assert.equal(m.ok, false);
  assert.match(m.problems.join('\n'), /node: caso duplicado/);
  assert.match(m.problems.join('\n'), new RegExp(`node · ${gone}: falta el fixture`));
});

test('a hash changed in an intermediate tick fails even when the final hash is the same', () => {
  const c = reference.cases.find((x) => x.name === 'test-loop.autopilot');
  const tick = 1234;
  const at = (tick - 1) * HEX_PER_TICK;
  const bad = c.trace.slice(0, at) + flip(c.trace.slice(at, at + HEX_PER_TICK)) + c.trace.slice(at + HEX_PER_TICK);
  assert.equal(bad.slice(-HEX_PER_TICK), c.trace.slice(-HEX_PER_TICK), 'same final hash');
  const envs = full();
  envs[4] = env('webkit', (r) => (r.case === c.name ? { ...r, trace: bad } : r));
  const m = matrix(envs);
  assert.equal(m.ok, false);
  assert.equal(m.cells[c.name].webkit.firstMismatch, tick);
  assert.ok(m.problems.some((p) => p.includes(`webkit · ${c.name}: desvío respecto de nativo en el tick ${tick}`)));
});

test('an incomplete trace fails', () => {
  const envs = full();
  envs[5] = env('safari', (r) => (r.case === 'test-loop.adversarial' ? { ...r, trace: r.trace.slice(0, -HEX_PER_TICK), ticks: r.ticks - 1 } : r));
  const m = matrix(envs);
  assert.equal(m.ok, false);
  assert.ok(m.cells['test-loop.adversarial'].safari.problems.some((p) => p.startsWith('incompleto')));
});

test('a missing native reference fails', () => {
  assert.equal(evaluateMatrix(full(), fixtures, undefined).ok, false);
});

test('invalid native traces fail without throwing or dropping the replay from the matrix', () => {
  const [first, ...rest] = reference.cases;
  for (const trace of [undefined, `${first.trace}a`, `g${first.trace.slice(1)}`]) {
    const m = matrix(full(), fixtures, { ...reference, cases: [{ ...first, trace }, ...rest] });
    assert.equal(m.ok, false);
    assert.match(m.cells[first.name].safari.problems.join('\n'), /traza nativa inválida/);
  }
  const malformed = matrix(full(), fixtures, { ...reference, cases: {} });
  assert.ok(malformed.problems.includes('estructura de la referencia nativa inválida'));
  assert.ok(malformed.cases.includes(first.name));
});

test('an environment trace with a non-hex digit fails even when its length matches', () => {
  const first = reference.cases[0];
  const m = evaluateMatrix([env('node', (r) => (r.case === first.name ? { ...r, trace: `g${r.trace.slice(1)}` } : r))],
    fixtures, reference, { requiredEnvs: ['node'] });
  assert.equal(m.ok, false);
  assert.match(m.cells[first.name].node.problems.join('\n'), /traza del entorno inválida/);
});

test('fixture checks must be consumed in order and match their ticks', () => {
  const f = fixtures.find((x) => x.name === 'test-loop.scripted');
  const trace = reference.cases.find((x) => x.name === f.name).trace;
  assert.deepEqual(checkFixture(f, trace), []);
  const swapped = { ...f, checks: [f.checks[1], f.checks[0], ...f.checks.slice(2)] };
  assert.match(checkFixture(swapped, trace)[0], /fuera de orden/);
  const wrong = { ...f, checks: [{ ...f.checks[0], hash: flip(f.checks[0].hash) }, ...f.checks.slice(1)] };
  assert.match(checkFixture(wrong, trace)[0], /control del tick/);
});

test('firstDivergence counts a missing tick as different', () => {
  assert.equal(firstDivergence('a'.repeat(32), 'a'.repeat(32)), null);
  assert.equal(firstDivergence('a'.repeat(32), 'a'.repeat(16)), 2);
  assert.equal(firstDivergence('', 'a'.repeat(16)), 1);
});

test('the CLI writes the step summary and keeps a failing exit code', () => {
  const dir = mkdtempSync(join(tmpdir(), 'rumbo-summary-'));
  try {
    const write = (envs) => {
      rmSync(join(dir, 'results'), { recursive: true, force: true });
      for (const e of envs) {
        mkdirSync(join(dir, 'results', `results-${e.browser}`), { recursive: true });
        writeFileSync(join(dir, 'results', `results-${e.browser}`, `results-${e.browser}.json`), JSON.stringify(e));
      }
    };
    const run = () => {
      const out = join(dir, 'summary.md');
      rmSync(out, { force: true });
      const r = spawnSync(process.execPath, [join(here, 'summary.mjs'), join(dir, 'results'), '--artifacts', '--reference', referencePath, '--step-summary', out], { encoding: 'utf8' });
      return { status: r.status, md: readFileSync(out, 'utf8') };
    };
    write(full());
    const ok = run();
    assert.equal(ok.status, 0);
    assert.match(ok.md, /Cero diferencias/);
    write(full().filter((e) => e.browser !== 'edge'));
    const bad = run();
    assert.equal(bad.status, 1);
    assert.match(bad.md, /falta el entorno edge/);
    assert.match(bad.md, /falta el artefacto results-edge/);

    write(full());
    mkdirSync(join(dir, 'results', 'results-extra'));
    writeFileSync(join(dir, 'results', 'results-extra', 'log.txt'), 'unexpected artifact');
    const unexpected = run();
    assert.equal(unexpected.status, 1);
    assert.match(unexpected.md, /artefacto inesperado: results-extra/);

    write(full());
    writeFileSync(join(dir, 'results', 'results-safari', 'results-safari.json'), JSON.stringify(env('edge')));
    const mislabeled = run();
    assert.equal(mislabeled.status, 1);
    assert.match(mislabeled.md, /entorno declarado edge, se esperaba safari/);
    assert.equal(summarize({ dir: join(dir, 'nada'), referencePath, root }).ok, false);
  } finally {
    rmSync(dir, { recursive: true, force: true });
  }
});
