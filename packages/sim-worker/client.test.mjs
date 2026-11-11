import assert from 'node:assert/strict';
import test from 'node:test';
import { createChallenge } from '@rumbo/contracts/testing';
import { createWorkerSim } from './dist/client.js';
for (const failure of ['onerror', 'onmessageerror', 'fatal', 'dispose']) {
  test(failure + ' rejects all pending hash requests and terminates once', async (t) => {
    let worker;
    let terminated = 0;
    const original = globalThis.Worker;
    globalThis.Worker = class {
      constructor() {
        worker = this;
      }
      postMessage() {}
      terminate() {
        terminated++;
      }
    };
    t.after(() => {
      if (original) globalThis.Worker = original;
      else delete globalThis.Worker;
    });
    const events = [];
    const port = createWorkerSim((msg) => events.push(msg));
    const a = port.hashContent(new Uint8Array(1)),
      b = port.hashContent(new Uint8Array(2));
    const rejected = Promise.all([assert.rejects(a), assert.rejects(b)]);
    if (failure === 'dispose') port.dispose();
    else if (failure === 'fatal')
      worker.onmessage({
        data: { type: 'fatal', code: 'SIM_FAILED', message: 'trap', generation: 0, protocolVersion: 1 },
      });
    else worker[failure]({ message: 'transport failed' });
    await rejected;
    await assert.rejects(port.hashContent(new Uint8Array(0)));
    assert.equal(events.length, failure === 'dispose' ? 0 : 1);
    port.dispose();
    assert.equal(terminated, 1);
    assert.equal((await port.encodeChallenge(createChallenge())).code, 'UNSUPPORTED_CAPABILITY');
    assert.equal((await port.decodeChallenge(new Uint8Array(0))).code, 'UNSUPPORTED_CAPABILITY');
  });
}
