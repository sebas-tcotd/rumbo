import test from 'node:test';
import assert from 'node:assert/strict';
import { ghostTicksAt, interpolationAlpha } from './dist/index.js';

test('alpha is the elapsed fraction of one tick since the snapshot was due', () => {
  const at = 1_700_000_000_000;
  const tick = 1000 / 120;
  assert.equal(interpolationAlpha(at, at, 120), 0);
  assert.ok(Math.abs(interpolationAlpha(at + tick, at, 120) - 1) < 1e-4, 'one full tick');
  assert.ok(Math.abs(interpolationAlpha(at + tick / 2, at, 120) - 0.5) < 1e-4);
  assert.equal(interpolationAlpha(at + 5 * tick, at, 120), 1, 'clamped above');
  assert.equal(interpolationAlpha(at - 3, at, 120), 0, 'a future `at` clamps to 0');
  assert.equal(interpolationAlpha(Number.NaN, at, 120), 1);
});

test('ghostTicksAt finds the first frame at or past s', () => {
  const frames = new Float32Array(8 * 4);
  [0, 10, 20, 30].forEach((s, i) => {
    frames[i * 8 + 7] = s;
  });
  assert.equal(ghostTicksAt(frames, 0), 0);
  assert.equal(ghostTicksAt(frames, 15), 2);
  assert.equal(ghostTicksAt(frames, 20), 2);
  assert.equal(ghostTicksAt(frames, 99), 3);
  assert.equal(ghostTicksAt(null, 5), null);
  assert.equal(ghostTicksAt(frames.subarray(0, 8), 5), null);
});
