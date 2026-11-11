import assert from 'node:assert/strict';
import test from 'node:test';
import { execFileSync, spawnSync } from 'node:child_process';
import { readFileSync } from 'node:fs';
import { join } from 'node:path';
import { fileURLToPath } from 'node:url';
import init, { Lab, default_profile, hash_content, snapshot_descriptor } from '@rumbo/sim-wasm';
import {
  STATE_FIELDS,
  STATE_BYTES,
  SNAPSHOT_VERSION,
  PROTOCOL_VERSION,
  DEV_SEEK_MAX_TICKS,
  protocolDescriptor,
  validateSnapshotDescriptor,
  isCommand,
  isResponse,
  isReplayHeader,
  isLinkHeader,
} from '@rumbo/contracts';
import { createChallenge, createCommand, createFatal } from '@rumbo/contracts/testing';

const root = fileURLToPath(new URL('../../', import.meta.url));
const read = (path) => readFileSync(join(root, path));
// Real on both sides: layout, bits, input word, RTF envelope, BLAKE3 hash (Rust types and WASM binding).
// Prospective: protocol, header, link and codec compare names and shapes against hand-written Rust fixtures
// because their Rust types do not exist yet (0.10 / 6.7). See crates/sim-wasm/examples/wire_parity.rs.
test('native Rust and TypeScript independently produce and reject parity fixtures', async () => {
  execFileSync('cargo', ['build', '--release', '--locked', '-p', 'sim-wasm', '--example', 'wire_parity'], {
    cwd: root,
  });
  const peer = join(
    root,
    'target/release/examples/wire_parity' + (process.platform === 'win32' ? '.exe' : ''),
  );
  const native = JSON.parse(execFileSync(peer, { encoding: 'utf8' }));
  const wasm = await init({ module_or_path: read('crates/sim-wasm/pkg/sim_wasm_bg.wasm') });
  const lab = new Lab(read('assets/tracks/test-loop.json').toString(), default_profile(), 7);
  try {
    lab.step(-31, 200, 3, 1);
    const state = lab.state();
    const ptr = lab.state_ptr();
    assert.deepEqual(new Float32Array(wasm.memory.buffer, ptr, STATE_FIELDS.length), state);
    const bits = [...new Uint32Array(state.buffer, state.byteOffset, state.length)];
    const layout = {
      version: SNAPSHOT_VERSION,
      stride: STATE_BYTES,
      fields: STATE_FIELDS.map((name, i) => ({ name, type: 'f32', offset: i * 4 })),
    };
    assert.deepEqual(JSON.parse(snapshot_descriptor()), layout);
    const base = { protocolVersion: PROTOCOL_VERSION, generation: 2 };
    // Frozen RTF v1 InputFrame fixture from the track-format codec.
    const envelope = [82, 84, 70, 0, 1, 0, 225, 200, 3, 1];
    const ts = {
      layout,
      bits,
      protocol: protocolDescriptor(),
      input: ((1 << 24) | (3 << 16) | (200 << 8) | 225) >>> 0,
      envelope,
      hash: hash_content(Uint8Array.from(envelope)),
      load: createCommand({ type: 'load', zone: null, race: null, track: 'track', profile: 'profile' }, 2),
      fatal: { ...base, ...createFatal('trap') },
      devTeleport: createCommand({ type: 'devTeleport', s: 120.5 }, 2),
      devSeekReplay: createCommand({ type: 'devSeekReplay', json: '{}', tick: DEV_SEEK_MAX_TICKS }, 2),
      header: {
        format: 'rumbo-replay/2',
        physicsVersion: '0.5.0',
        trackHash: 't',
        raceHash: 'r',
        profileRef: { id: 'p', hash: 'h' },
        seed: '18446744073709551615',
      },
      link: '#v1.i.YQ',
      codec: {
        encodeChallenge: Object.keys(createChallenge()),
        decodeChallenge: ['bytes'],
        hashContent: ['bytes'],
        routeMeta: createChallenge().routeMeta,
      },
    };
    assert.deepEqual(native, ts);
    validateSnapshotDescriptor(native.layout);
    for (const change of [{ name: 'bad' }, { type: 'f64' }, { offset: 1 }]) {
      const bad = structuredClone(native.layout);
      Object.assign(bad.fields[0], change);
      assert.throws(() => validateSnapshotDescriptor(bad));
    }
    assert.throws(() => validateSnapshotDescriptor({ ...native.layout, version: 9 }));
    assert.throws(() => validateSnapshotDescriptor({ ...native.layout, stride: 4 }));
    const check = (value) => spawnSync(peer, ['check'], { input: JSON.stringify(value) }).status;
    assert.equal(check(ts), 0);
    for (const key of Object.keys(ts)) assert.notEqual(check({ ...ts, [key]: null }), 0, key);
    for (const [key, reader] of [
      ['load', isCommand],
      ['devTeleport', isCommand],
      ['devSeekReplay', isCommand],
      ['fatal', isResponse],
      ['header', isReplayHeader],
      ['link', isLinkHeader],
    ]) {
      assert.ok(reader(native[key]));
      if (key === 'link') {
        for (const bad of ['#v2.i.a', '#v1.r.a', '#v1.i.']) assert.equal(reader(bad), false);
      } else
        for (const field of Object.keys(native[key])) {
          const bad = { ...native[key] };
          delete bad[field];
          assert.equal(reader(bad), false, key + '.' + field);
        }
    }
    // Dev command bounds: one tick past the shared limit, a non-finite s and an oversized replay are rejected.
    assert.equal(isCommand({ ...native.devSeekReplay, tick: DEV_SEEK_MAX_TICKS + 1 }), false);
    assert.equal(isCommand({ ...native.devSeekReplay, tick: 1.5 }), false);
    assert.equal(isCommand({ ...native.devTeleport, s: Number.POSITIVE_INFINITY }), false);
    assert.equal(isCommand({ ...native.devSeekReplay, json: 'x'.repeat(16 * 1024 * 1024 + 1) }), false);
  } finally {
    lab.free();
  }
});
