import assert from 'node:assert/strict';
import { test } from 'node:test';
import { ERROR_CATALOG, err, ok, SIM_EDGE_ERROR_CODES } from './dist/index.js';

test('Result helpers preserve the value and the structured failure', () => {
  assert.deepEqual(ok(undefined), { ok: true, value: undefined });
  assert.deepEqual(err({ code: 'STORAGE_UNAVAILABLE' }), {
    ok: false,
    error: { code: 'STORAGE_UNAVAILABLE' },
  });
});

test('visible errors have stable severity levels and all WASM edge codes are catalogued', () => {
  assert.equal(ERROR_CATALOG.WASM_LOAD_FAILED, 'fatal');
  assert.equal(ERROR_CATALOG.STORAGE_UNAVAILABLE, 'recoverable');
  assert.equal(ERROR_CATALOG.GAMEPAD_UNAVAILABLE, 'degraded');
  assert.equal(ERROR_CATALOG.INVALID_REPLAY, 'invalidAttempt');
  assert.deepEqual(SIM_EDGE_ERROR_CODES, [
    'INVALID_TRACK', 'INVALID_PROFILE', 'INVALID_REPLAY', 'EXPORT_FAILED', 'SCHEMA_FAILED',
  ]);
  for (const code of SIM_EDGE_ERROR_CODES) assert.ok(Object.hasOwn(ERROR_CATALOG, code));
});
