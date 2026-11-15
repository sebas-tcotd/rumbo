// Story 0.9 · Pure evaluation of the determinism matrix: one result per case and environment, compared
// tick by tick against the native reference and against the fixture's own checks. No hashing here: traces
// are compared as the lossless hex strings the Rust harness produced (16 hex digits per tick).
import { REQUIRED_CASES } from './cases.mjs';

export const HEX_PER_TICK = 16;

/** Node plus the five browsers of S4 (D40). Every one must report, no more, no fewer. */
export const REQUIRED_ENVS = Object.freeze(['node', 'chromium', 'edge', 'firefox', 'webkit', 'safari']);

/** Hash of tick `t` (1-based: the state after the t-th step) in a trace, or undefined. */
export function hashAt(trace, t) {
  const h = trace.slice((t - 1) * HEX_PER_TICK, t * HEX_PER_TICK);
  return h.length === HEX_PER_TICK ? h : undefined;
}

/** First tick (1-based) whose hash differs between two traces, counting a missing tick as different; null if equal. */
export function firstDivergence(a, b) {
  const n = Math.max(a.length, b.length) / HEX_PER_TICK;
  for (let t = 1; t <= Math.ceil(n); t++) if (hashAt(a, t) !== hashAt(b, t)) return t;
  return null;
}

/**
 * The fixture's control hashes must be consumed in order and match the trace at their ticks; the last
 * trace hash must be the fixture's final hash. Returns a list of problems (empty when valid).
 */
export function checkFixture(fixture, trace) {
  const problems = [];
  let last = 0;
  for (const [i, c] of fixture.checks.entries()) {
    if (!Number.isInteger(c.tick) || c.tick <= last || c.tick > fixture.ticks) {
      problems.push(`control ${i} fuera de orden o de rango (tick ${c.tick})`);
      break;
    }
    last = c.tick;
    const h = hashAt(trace, c.tick);
    if (h !== c.hash) {
      problems.push(`control del tick ${c.tick}: esperado ${c.hash}, obtenido ${h ?? '—'}`);
      break;
    }
  }
  const final = hashAt(trace, fixture.ticks);
  if (final !== fixture.expected) problems.push(`hash final: esperado ${fixture.expected}, obtenido ${final ?? '—'}`);
  return problems;
}

/**
 * Evaluate one environment's result for one case.
 * @param fixture  `listCases` entry (name, ticks, expected, checks) or undefined if the fixture is missing.
 * @param reference `{ ticks, trace }` from the native reference, or undefined.
 * @param result   the environment's entry `{ case, report, ticks, trace, error }`, or undefined.
 * @returns `{ ok, problems, firstMismatch }`; firstMismatch is the first tick that differs from native.
 */
export function evaluateCase(fixture, reference, result) {
  const problems = [];
  let firstMismatch = null;
  if (!fixture) problems.push('falta el fixture');
  if (!reference) problems.push('falta en la referencia nativa');
  if (!result) return { ok: false, problems: [...problems, 'falta el resultado'], firstMismatch };
  if (result.error) problems.push(`error: ${result.error}`);
  if (!result.report?.ok) problems.push(`verificación heredada: ${result.report?.message ?? 'sin informe'}`);
  const trace = typeof result.trace === 'string' ? result.trace : '';
  if (fixture) {
    if (trace.length !== fixture.ticks * HEX_PER_TICK || result.ticks !== fixture.ticks) {
      problems.push(`incompleto: ${trace.length / HEX_PER_TICK} hashes para ${fixture.ticks} ticks declarados`);
    }
    if (reference && reference.ticks !== fixture.ticks) problems.push(`la referencia tiene ${reference.ticks} ticks, el fixture ${fixture.ticks}`);
    problems.push(...checkFixture(fixture, trace));
  }
  if (reference) {
    firstMismatch = firstDivergence(trace, reference.trace);
    if (firstMismatch !== null) problems.push(`desvío respecto de nativo en el tick ${firstMismatch}`);
  }
  return { ok: problems.length === 0, problems, firstMismatch };
}

/**
 * Evaluate the whole matrix.
 * @param envs      parsed `results-*.json` files (each `{ browser, userAgent, physics, error, results }`).
 * @param fixtures  `listCases(root)`.
 * @param reference parsed native reference `{ physicsVersion, cases: [{ name, ticks, trace }] }` or undefined.
 * @param opts      `{ requiredEnvs, requiredCases }` (defaults: the six environments and eight goldens).
 * @returns `{ ok, problems, cases, envs, cells }` where cells[case][env] = evaluateCase(...).
 */
export function evaluateMatrix(envs, fixtures, reference, opts = {}) {
  const requiredEnvs = opts.requiredEnvs ?? REQUIRED_ENVS;
  const requiredCases = opts.requiredCases ?? REQUIRED_CASES;
  const problems = [];
  if (!reference) problems.push('falta la referencia nativa');

  const byEnv = new Map();
  for (const e of envs) {
    const name = e?.browser;
    if (!requiredEnvs.includes(name)) problems.push(`entorno inesperado: ${name}`);
    if (byEnv.has(name)) problems.push(`entorno duplicado: ${name}`);
    else byEnv.set(name, e);
    if (e?.error) problems.push(`${name}: error: ${e.error}`);
    if (reference && e?.physics !== reference.physicsVersion) {
      problems.push(`${name}: física ${e?.physics ?? '?'}, referencia ${reference.physicsVersion}`);
    }
  }
  for (const name of requiredEnvs) if (!byEnv.has(name)) problems.push(`falta el entorno ${name}`);

  const fixtureBy = new Map(fixtures.map((f) => [f.name, f]));
  const refBy = new Map();
  for (const c of reference?.cases ?? []) {
    if (refBy.has(c.name)) problems.push(`caso duplicado en la referencia: ${c.name}`);
    refBy.set(c.name, c);
  }
  for (const f of fixtures) if (!requiredCases.includes(f.name)) problems.push(`fixture inesperado: ${f.name}`);
  for (const name of refBy.keys()) if (!requiredCases.includes(name)) problems.push(`caso inesperado en la referencia: ${name}`);

  const extraCases = [...new Set([...fixtureBy.keys(), ...refBy.keys()])].filter((n) => !requiredCases.includes(n));
  const cases = [...requiredCases, ...extraCases.sort()];
  const envNames = [...requiredEnvs, ...[...byEnv.keys()].filter((n) => !requiredEnvs.includes(n))];
  const cells = {};
  for (const c of cases) cells[c] = {};

  for (const env of envNames) {
    const e = byEnv.get(env);
    const results = Array.isArray(e?.results) ? e.results : [];
    const seen = new Map();
    for (const r of results) {
      if (seen.has(r?.case)) problems.push(`${env}: caso duplicado ${r?.case}`);
      else seen.set(r?.case, r);
      if (!cases.includes(r?.case)) problems.push(`${env}: caso inesperado ${r?.case}`);
    }
    for (const c of cases) {
      const cell = e ? evaluateCase(fixtureBy.get(c), refBy.get(c), seen.get(c)) : { ok: false, problems: ['falta el entorno'], firstMismatch: null };
      cells[c][env] = cell;
      // A missing environment is already reported once above.
      if (!cell.ok && e) problems.push(...cell.problems.map((p) => `${env} · ${c}: ${p}`));
    }
  }
  return { ok: problems.length === 0, problems, cases, envs: envNames, cells };
}
