import test from 'node:test';
import assert from 'node:assert/strict';
import { sampleIndexAt } from './sample-index.ts';

test('circuits wrap the lap distance, sprints clamp it', () => {
  assert.equal(sampleIndexAt(0, 2, 100, true), 0);
  assert.equal(sampleIndexAt(21, 2, 100, true), 11);
  assert.equal(sampleIndexAt(-2, 2, 100, true), 99);
  assert.equal(sampleIndexAt(202, 2, 100, true), 1);
  assert.equal(sampleIndexAt(-50, 2, 100, false), 0);
  assert.equal(sampleIndexAt(500, 2, 100, false), 99);
});
