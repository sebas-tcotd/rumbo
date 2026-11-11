import test from 'node:test';
import assert from 'node:assert/strict';
import { SnapshotPool } from './pool.ts';
test('three held slots exhaust the pool and real transfers detach both senders', () => {
  const pool = new SnapshotPool(328);
  const held = Array.from({ length: 3 }, () => pool.take());
  assert.equal(pool.take(), null);
  let { slot, buffer } = held[1];
  for (let i = 0; i < 3000; i++) {
    const host = structuredClone(buffer, { transfer: [buffer] });
    assert.equal(buffer.byteLength, 0);
    assert.equal(pool.take(), null);
    pool.release(slot, structuredClone(host, { transfer: [host] }));
    assert.equal(host.byteLength, 0);
    ({ slot, buffer } = pool.take());
  }
});
