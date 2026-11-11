import test from 'node:test';
import assert from 'node:assert/strict';
import { FixedClock } from './clock.ts';
test('caps debt, retains fractional time and discards paused time', () => {
  const clock = new FixedClock(120);
  clock.start(0);
  assert.equal(clock.advance(0), 0);
  assert.equal(clock.advance(12), 1);
  assert.equal(clock.advance(1012), 8);
  assert.equal(clock.advance(1017), 1);
  clock.stop();
  assert.equal(clock.advance(5000), 0);
  clock.start(6000);
  assert.equal(clock.advance(6001), 0);
});

test('long pause and resume never produce catch-up ticks or variable steps', () => {
  const clock = new FixedClock(120);
  clock.start(0);
  assert.equal(clock.advance(8), 0);
  clock.stop();
  assert.equal(clock.advance(3_600_000), 0);
  clock.start(3_600_000);
  assert.equal(clock.advance(3_600_000 + 8), 0);
  assert.equal(clock.advance(3_600_000 + 9), 1);
});

test('zero, one, exactly eight and more than eight due steps', () => {
  const step = 1000 / 120;
  for (const [elapsed, expected] of [
    [0, 0],
    [step, 1],
    [step * 8, 8],
    [step * 20, 8],
  ]) {
    const clock = new FixedClock(120);
    clock.start(0);
    assert.equal(clock.advance(elapsed + 1e-9), expected, String(elapsed));
    assert.ok(clock.remainder < step, 'discarded debt is not carried over');
  }
});

test('only 120 Hz is accepted', () => {
  assert.throws(() => new FixedClock(60), /UNSUPPORTED_TICK_RATE/);
});
