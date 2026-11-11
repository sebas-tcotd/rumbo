import assert from 'node:assert/strict';
import test from 'node:test';
import { countWarnings, summary } from './pedantic-summary.mjs';

const warning = (code, line, level = 'warning') =>
  JSON.stringify({
    reason: 'compiler-message',
    message: { level, code: { code }, spans: [{ file_name: 'src/a.rs', line_start: line, column_start: 1 }] },
  });

test('counts warnings by lint, skips errors, notes without code and duplicate targets', () => {
  const text = [
    warning('clippy::cast_sign_loss', 1),
    warning('clippy::cast_sign_loss', 1), // same span reported again for the test target
    warning('clippy::cast_sign_loss', 2),
    warning('clippy::must_use_candidate', 3),
    warning('clippy::unwrap_used', 4, 'error'),
    JSON.stringify({ reason: 'compiler-message', message: { level: 'warning', code: null, spans: [] } }),
    JSON.stringify({ reason: 'build-finished', success: true }),
    'Checking sim-core',
  ].join('\n');
  const counts = countWarnings(text);
  assert.deepEqual([...counts], [['clippy::cast_sign_loss', 2], ['clippy::must_use_candidate', 1]]);
  assert.match(summary(counts), /not blocking\): 3\n[\s\S]*\| `clippy::cast_sign_loss` \| 2 \|/);
});
