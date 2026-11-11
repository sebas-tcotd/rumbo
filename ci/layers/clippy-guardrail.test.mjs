// Guardrail (ADR-005, P1): intentional violations must make the blocking Clippy run of CI fail with
// the expected lint, and pedantic must stay a warning. Each case writes a throwaway example into a
// real crate (Cargo discovers examples without touching Cargo.toml), runs Clippy on just that example
// and always removes the file.
//
// Slow (it compiles the crate under Clippy): run on its own with `pnpm --filter rumbo-layers
// test:clippy`, never inside `turbo run test`, which may be compiling the same crates in parallel.

import assert from 'node:assert/strict';
import { spawnSync } from 'node:child_process';
import { existsSync, mkdirSync, rmSync, writeFileSync } from 'node:fs';
import { join } from 'node:path';
import test from 'node:test';
import { fileURLToPath } from 'node:url';

const ROOT = fileURLToPath(new URL('../../', import.meta.url));

/** The blocking flags of the CI clippy step (.github/workflows/ci.yml, job layers). */
export const BLOCKING = ['-D', 'warnings', '-A', 'clippy::pedantic'];

/** Runs Clippy on one throwaway example; returns { status, lints: Map<code, level[]> }. */
function clippyExample(crate, source, flags = BLOCKING) {
  const name = `zz_guardrail_${process.pid}`;
  const dir = join(ROOT, 'crates', crate, 'examples');
  const createdDir = !existsSync(dir);
  const path = join(dir, name + '.rs');
  try {
    mkdirSync(dir, { recursive: true });
    writeFileSync(path, source);
    const args = ['clippy', '-p', crate, '--example', name, '--locked', '--message-format=json', '--', ...flags];
    const run = spawnSync('cargo', args, { cwd: ROOT, encoding: 'utf8', maxBuffer: 64 * 1024 * 1024 });
    return { status: run.status, lints: lintsOf(run.stdout, name), stderr: run.stderr };
  } finally {
    rmSync(createdDir ? dir : path, { recursive: true, force: true });
  }
}

/** Lint codes reported for the probe file, with their levels. */
function lintsOf(stdout, name) {
  const lints = new Map();
  for (const line of stdout.split('\n')) {
    if (!line.startsWith('{')) continue;
    const message = JSON.parse(line);
    const diagnostic = message.reason === 'compiler-message' ? message.message : null;
    const code = diagnostic?.code?.code;
    if (!code || !diagnostic.spans.some((span) => span.file_name.includes(name))) continue;
    lints.set(code, [...(lints.get(code) ?? []), diagnostic.level]);
  }
  return lints;
}

function assertDenied(result, code) {
  assert.notEqual(result.status, 0, 'clippy must fail\n' + result.stderr.slice(-2000));
  assert.ok(result.lints.get(code)?.includes('error'), `${code} must be an error; got ${JSON.stringify([...result.lints])}`);
}

const CORE_VIOLATIONS = `
use std::collections::HashMap;

#[allow(dead_code)]
fn unused() {}

fn main() {
    let mut cars: HashMap<u32, u32> = HashMap::new();
    cars.insert(1, 2);
    let started = std::time::Instant::now();
    let file = std::fs::read("track.json");
    let angle = 1.0_f64.sin();
    let square = 2.0_f32.powi(2);
    let first = Some(1).unwrap();
    println!("{} {:?} {} {} {} {}", cars.len(), started, file.is_ok(), angle, square, first);
}
`;

test('sim-core: core rules, unwrap and #[allow] without reason are errors', () => {
  const result = clippyExample('sim-core', CORE_VIOLATIONS);
  for (const code of [
    'clippy::disallowed_types',
    'clippy::disallowed_methods',
    'clippy::unwrap_used',
    'clippy::allow_attributes_without_reason',
  ]) {
    assertDenied(result, code);
  }
});

test('track-compiler-core: its own clippy.toml is live (float and collection rules)', () => {
  const source = 'fn main() {\n    let set: std::collections::HashSet<u8> = std::collections::HashSet::new();\n    println!("{} {}", set.len(), 3.0_f64.powi(2));\n}\n';
  const result = clippyExample('track-compiler-core', source);
  assertDenied(result, 'clippy::disallowed_types');
  assertDenied(result, 'clippy::disallowed_methods');
});

test('track-format: its own clippy.toml is live', () => {
  const result = clippyExample('track-format', 'fn main() {\n    println!("{:?}", std::time::SystemTime::now());\n}\n');
  assertDenied(result, 'clippy::disallowed_methods');
});

test('outside the core: HashMap is allowed, unwrap is still an error', () => {
  const source = 'fn main() {\n    let mut m = std::collections::HashMap::new();\n    m.insert(1, 2);\n    println!("{}", m.get(&1).copied().unwrap());\n}\n';
  const result = clippyExample('compiler-cli', source);
  assertDenied(result, 'clippy::unwrap_used');
  assert.equal(result.lints.has('clippy::disallowed_types'), false, 'core rules must not reach adapter crates');
});

const CLEAN_WITH_PEDANTIC = `
/// Allowed in the core: IEEE-exact float methods, libm and BTreeMap.
fn main() {
    let mut cars = std::collections::BTreeMap::new();
    cars.insert(1_u32, 2_u32);
    let ticks: u64 = 7;
    #[allow(clippy::cast_possible_truncation, reason = "guardrail control: a lint allowed with its reason")]
    let small = ticks as u8;
    let seconds = ticks as f64 / 120.0; // cast_precision_loss: pedantic
    let length = (seconds * seconds).sqrt().max(0.0).rem_euclid(1.0);
    println!("{} {} {} {}", cars.len(), small, length, libm::sin(seconds));
}
`;

test('clean core code passes the blocking run; pedantic stays a warning', () => {
  const blocking = clippyExample('sim-core', CLEAN_WITH_PEDANTIC);
  assert.equal(blocking.status, 0, 'clean code must pass the blocking run\n' + blocking.stderr.slice(-2000));
  const report = clippyExample('sim-core', CLEAN_WITH_PEDANTIC, []);
  assert.equal(report.status, 0, 'the pedantic report must not fail\n' + report.stderr.slice(-2000));
  assert.deepEqual(report.lints.get('clippy::cast_precision_loss'), ['warning']);
});
