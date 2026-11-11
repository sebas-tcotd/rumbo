import test from 'node:test';
import assert from 'node:assert/strict';
import { formatLap } from './format-lap.ts';

test('formats ticks as m:ss.mmm and dashes missing times', () => {
  assert.equal(formatLap(0, 120), '0:00.000');
  assert.equal(formatLap(120 * 61.5, 120), '1:01.500');
  assert.equal(formatLap(-1, 120), '—');
  assert.equal(formatLap(Number.NaN, 120), '—');
});
