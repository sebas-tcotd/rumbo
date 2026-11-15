// Story 0.9: the WASM budget check passes at the limit, fails above it, fails without a binary and fails if
// the harness export leaks into the publication build. Uses temporary hand-made modules.
import assert from 'node:assert/strict';
import { mkdirSync, mkdtempSync, rmSync, writeFileSync } from 'node:fs';
import { tmpdir } from 'node:os';
import { join } from 'node:path';
import test from 'node:test';
import { checkWasm, gzipSize, LIMIT } from './check-wasm-size.mjs';

/** Minimal valid module exporting a memory under each name (WASM binary format, sections 5 and 7). */
function moduleWithExports(names = [], padding = 0) {
  const enc = new TextEncoder();
  const bytes = [0x00, 0x61, 0x73, 0x6d, 0x01, 0x00, 0x00, 0x00];
  if (names.length) {
    bytes.push(0x05, 0x03, 0x01, 0x00, 0x00); // memory section: one memory, min 0
    const body = [names.length];
    for (const n of names) body.push(n.length, ...enc.encode(n), 0x02, 0x00); // export memory 0
    bytes.push(0x07, body.length, ...body);
  }
  if (padding) {
    // Custom section "pad" with deterministic pseudo-random bytes (so gzip cannot shrink it away).
    const name = enc.encode('pad');
    const payload = [name.length, ...name];
    let x = 7;
    for (let i = 0; i < padding; i++) payload.push((x = (x * 1103515245 + 12345) >>> 0) >>> 24);
    const size = [];
    let n = payload.length;
    do {
      size.push((n & 0x7f) | (n > 0x7f ? 0x80 : 0));
      n >>>= 7;
    } while (n);
    bytes.push(0x00, ...size, ...payload);
  }
  return Uint8Array.from(bytes);
}

function dist(files) {
  const dir = mkdtempSync(join(tmpdir(), 'rumbo-wasm-'));
  mkdirSync(join(dir, 'assets'));
  for (const [name, bytes] of Object.entries(files)) writeFileSync(join(dir, 'assets', name), bytes);
  return dir;
}

test('the budget is 1 500 000 gzip bytes, inclusive', () => {
  assert.equal(LIMIT, 1_500_000);
});

test('a binary exactly at the limit passes and one byte over fails', () => {
  const wasm = moduleWithExports(['memory'], 4000);
  const size = gzipSize(wasm);
  const dir = dist({ 'sim_wasm_bg-abc123.wasm': wasm });
  try {
    const at = checkWasm(dir, size);
    assert.equal(at.ok, true, at.problems.join('\n'));
    assert.equal(at.report[0].gzip, size);
    assert.equal(at.report[0].raw, wasm.length);
    assert.match(at.report[0].path, /sim_wasm_bg-abc123\.wasm$/);
    const over = checkWasm(dir, size - 1);
    assert.equal(over.ok, false);
    assert.match(over.problems[0], /superan el tope/);
  } finally {
    rmSync(dir, { recursive: true, force: true });
  }
});

test('a missing binary or a missing dist fails', () => {
  const dir = dist({ 'other.js': new Uint8Array([1]) });
  try {
    assert.equal(checkWasm(dir).ok, false);
    assert.equal(checkWasm(join(dir, 'nope')).ok, false);
  } finally {
    rmSync(dir, { recursive: true, force: true });
  }
});

test('two simulation binaries fail', () => {
  const dir = dist({ 'sim_wasm_bg-a.wasm': moduleWithExports(), 'sim_wasm_bg-b.wasm': moduleWithExports() });
  try {
    assert.match(checkWasm(dir).problems.join('\n'), /más de un WASM/);
  } finally {
    rmSync(dir, { recursive: true, force: true });
  }
});

test('the harness export in the publication binary fails', () => {
  const dir = dist({ 'sim_wasm_bg-x.wasm': moduleWithExports(['memory', 'trace_replay']) });
  try {
    const r = checkWasm(dir);
    assert.equal(r.ok, false);
    assert.match(r.problems.join('\n'), /exporta el arnés \(trace_replay\)/);
  } finally {
    rmSync(dir, { recursive: true, force: true });
  }
});

test('an invalid module fails', () => {
  const dir = dist({ 'sim_wasm_bg-x.wasm': new Uint8Array([0, 1, 2, 3]) });
  try {
    assert.match(checkWasm(dir).problems.join('\n'), /no es un módulo WebAssembly válido/);
  } finally {
    rmSync(dir, { recursive: true, force: true });
  }
});
