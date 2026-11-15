import assert from 'node:assert/strict';
import { test } from 'node:test';
import { createCounterRing, createLogger } from './dist/index.js';

test('the log ring overwrites its oldest entry and filters the publication console', () => {
  const output = [];
  const sink = Object.fromEntries(['error', 'warn', 'info', 'debug'].map((level) => [level, (...args) => output.push(args)]));
  const logger = createLogger({ capacity: 2, consoleLevel: 'warn', sink, now: () => 12 });
  logger.log('info', 'game', 'START');
  logger.log('warn', 'input', 'GAMEPAD_UNAVAILABLE', { error: new Error('blocked'), missing: ['pad'] });
  logger.log('debug', 'sim', 'READY');
  logger.log('trace', 'sim', 'SKIP');
  assert.deepEqual(logger.records().map((record) => record.code), ['GAMEPAD_UNAVAILABLE', 'READY']);
  assert.deepEqual(output, [['[input] GAMEPAD_UNAVAILABLE', { error: 'blocked', missing: 'pad' }]]);
  assert.throws(() => logger.log('warn', 'x', 'VALID'), TypeError);
  assert.throws(() => logger.log('warn', 'input', 'bad'), TypeError);
});

test('development logs debug and trace and counters retain the latest samples', () => {
  const output = [];
  const logger = createLogger({ capacity: 2, consoleLevel: 'debug', sink: { debug: (...args) => output.push(args) }, now: () => 0 });
  logger.log('trace', 'render', 'TRACE');
  assert.equal(output.length, 1);
  const ring = createCounterRing({ capacity: 2 });
  for (const t of [1000, 2000, 3000]) ring.push({ t, simMs: 1, simHz: 120, frameMs: 16 });
  assert.deepEqual(ring.samples().map(({ t }) => t), [2000, 3000]);
});

test('an unprintable error detail still produces a log record', () => {
  const logger = createLogger({ capacity: 1, consoleLevel: 'error', sink: { error() {} }, now: () => 0 });
  logger.log('error', 'sim', 'SIM_FAILED', { detail: Object.create(null) });
  assert.equal(logger.records()[0].fields.detail, '<unavailable>');
});
