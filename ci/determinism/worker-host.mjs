// Node host for the actual Vite worker bundle. Controlled time and binding faults are test-only.
import { parentPort, workerData } from 'node:worker_threads';
import { readFileSync } from 'node:fs';
globalThis.self = globalThis;
globalThis.postMessage = (msg, transfer) => parentPort.postMessage(msg, transfer);
let closed = false;
let now = 0;
let callback;
globalThis.close = () => {
  closed = true;
  callback = undefined;
};
Object.defineProperty(globalThis, 'performance', { value: { now: () => now, timeOrigin: 0 } });
globalThis.setTimeout = (fn) => {
  if (callback) throw Error('duplicate scheduler');
  callback = fn;
  return 1;
};
globalThis.clearTimeout = () => {
  callback = undefined;
};
let allowInit;
const initGate = workerData.deferInit
  ? new Promise((resolve) => {
      allowInit = resolve;
    })
  : Promise.resolve();
globalThis.fetch = async () => {
  await initGate;
  return new Response(readFileSync(workerData.wasm), { headers: { 'Content-Type': 'application/wasm' } });
};
if (workerData.fault) {
  const trap = new WebAssembly.Instance(
    new WebAssembly.Module(
      Uint8Array.from([
        0, 97, 115, 109, 1, 0, 0, 0, 1, 4, 1, 96, 0, 0, 3, 2, 1, 0, 7, 8, 1, 4, 116, 114, 97, 112, 0, 0, 10,
        5, 1, 3, 0, 0, 11,
      ]),
    ),
  ).exports.trap;
  const instantiate = WebAssembly.instantiateStreaming.bind(WebAssembly);
  WebAssembly.instantiateStreaming = async (...args) => {
    const result = await instantiate(...args);
    return { ...result, instance: { exports: { ...result.instance.exports, [workerData.fault]: trap } } };
  };
}
await import(workerData.bundle);
parentPort.on('message', async (msg) => {
  if (!closed) {
    if (msg.duringInit) {
      const first = self.onmessage({ data: msg.first });
      const second = self.onmessage({ data: msg.duringInit });
      allowInit();
      await Promise.all([first, second]);
    } else if (msg.wakes !== undefined) {
      for (let i = 0; i < msg.wakes; i++) {
        now += msg.elapsed;
        const next = callback;
        callback = undefined;
        next?.();
      }
    } else await self.onmessage({ data: msg });
  }
  parentPort.postMessage({ type: 'ack' });
});
