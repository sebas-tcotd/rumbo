import assert from 'node:assert/strict';
import { test } from 'node:test';
import { parseEdgeError } from './dist/edge-error.js';

test('only known WASM rejection prefixes are stripped', () => {
  assert.deepEqual(parseEdgeError('INVALID_REPLAY: invalid: format'), {
    code: 'INVALID_REPLAY',
    detail: 'invalid: format',
  });
  assert.equal(parseEdgeError('SOME_OTHER_CODE: detail'), null);
  assert.equal(parseEdgeError(new WebAssembly.RuntimeError('INVALID_REPLAY: trap')), null);
});
