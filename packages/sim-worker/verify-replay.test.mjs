import assert from 'node:assert/strict';
import { readFileSync } from 'node:fs';
import test from 'node:test';
import init, { Lab, default_profile, verify_replay } from '../../crates/sim-wasm/pkg/sim_wasm.js';
import { verifyReplay } from './verify-replay.ts';

const read = (path) => readFileSync(new URL(path, import.meta.url));
await init({ module_or_path: read('../../crates/sim-wasm/pkg/sim_wasm_bg.wasm') });
const track = read('../../assets/tracks/test-loop.json').toString();

test('a malformed replay allows retry on the same live session', () => {
  const lab = new Lab(track, default_profile(), 1);
  try {
    lab.step(0, 0, 0, 0);
    assert.equal(verifyReplay(verify_replay, track, '{').code, 'INVALID_REPLAY');
    lab.step(0, 0, 0, 0);
    assert.equal(lab.tick(), 2);
    assert.equal(verifyReplay(verify_replay, track, lab.replay_json()).report.ok, true);
  } finally {
    lab.free();
  }
});

test('a WebAssembly trap reaches the terminal failure handler', async () => {
  // A test-only module exporting a function whose sole instruction is unreachable.
  const binary = Uint8Array.from([
    0, 97, 115, 109, 1, 0, 0, 0, 1, 4, 1, 96, 0, 0, 3, 2, 1, 0, 7, 8, 1, 4, 116, 114, 97, 112, 0, 0, 10, 5, 1,
    3, 0, 0, 11,
  ]);
  const { instance } = await WebAssembly.instantiate(binary);
  assert.throws(() => verifyReplay(instance.exports.trap, track, '{}'), WebAssembly.RuntimeError);
});
