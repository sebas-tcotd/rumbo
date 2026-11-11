import test from 'node:test';
import assert from 'node:assert/strict';
import { buildIndex, STATE_FIELDS } from './snapshot.ts';
test('validates every state field in ABI order', () => {
  assert.equal(buildIndex([...STATE_FIELDS]).tick, 0);
  for (const layout of [
    STATE_FIELDS.slice(1),
    [...STATE_FIELDS, 'tick'],
    [...STATE_FIELDS].reverse(),
    STATE_FIELDS.map(() => 'tick'),
  ])
    assert.throws(() => buildIndex(layout));
});
