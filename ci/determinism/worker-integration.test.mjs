import assert from 'node:assert/strict';
import test from 'node:test';
import { Worker } from 'node:worker_threads';
import { readFileSync, readdirSync } from 'node:fs';
import { fileURLToPath } from 'node:url';

const assets = new URL('../../apps/web/dist/assets/', import.meta.url);
const files = readdirSync(assets);
const bundle = new URL(
  files.find((f) => /^worker-.*\.js$/.test(f)),
  assets,
).href;
const wasm = fileURLToPath(
  new URL(
    files.find((f) => f.endsWith('.wasm')),
    assets,
  ),
);
const track = readFileSync(new URL('../../assets/tracks/test-loop.json', import.meta.url), 'utf8');
const TICKS = 1800 * 8;
const load = { type: 'load', zone: null, race: null, track, profile: '' };

function host(t, fault, deferInit = false) {
  const worker = new Worker(new URL('./worker-host.mjs', import.meta.url), {
    workerData: { bundle, wasm, fault, deferInit },
  });
  t.after(() => worker.terminate());
  return async (command, transfer = []) => {
    const messages = [];
    const done = new Promise((resolve, reject) => {
      const onError = (err) => {
        cleanup();
        reject(err);
      };
      const onMessage = (msg) => {
        if (msg.type === 'ack') {
          cleanup();
          resolve();
        } else messages.push(msg);
      };
      function cleanup() {
        worker.off('message', onMessage);
        worker.off('error', onError);
      }
      worker.on('message', onMessage);
      worker.on('error', onError);
    });
    worker.postMessage({ protocolVersion: 1, generation: 1, ...command }, transfer);
    await done;
    return messages;
  };
}

test('real worker keeps racing with three slots held, publishes lap and resumes latest frame', async (t) => {
  const send = host(t);
  assert.deepEqual(
    (await send(load)).map((m) => m.type),
    ['session', 'ready'],
  );
  await send({ type: 'autopilot', on: true });
  await send({ type: 'start' });
  await send({ type: 'start' });
  const batch = await send({ wakes: 1800, elapsed: 100 });
  const held = batch.filter((m) => m.type === 'snapshot');
  assert.equal(held.length, 3);
  const view = (m) => new Float32Array(m.buffer);
  assert.deepEqual(
    held.map((m) => view(m)[41]),
    [8, 16, 24],
  );
  const ghost = batch.find((m) => m.type === 'lap' && m.best)?.ghost;
  assert.ok(ghost instanceof Float32Array && ghost.length > 8);
  const first = ghost[0];
  const { slot, buffer } = held[0];
  assert.ok(buffer.byteLength > 0);
  // The returned slot immediately carries the latest executed state that had no free slot.
  const [republished] = await send({ type: 'returnBuffer', slot, buffer }, [buffer]);
  assert.equal(republished.type, 'snapshot');
  assert.equal(view(republished)[41], TICKS);
  assert.equal(view(republished)[0], TICKS - 1);
  await send({ type: 'returnBuffer', slot: republished.slot, buffer: republished.buffer }, [republished.buffer]);
  const [latest] = await send({ wakes: 1, elapsed: 100 });
  assert.equal(view(latest)[41], TICKS + 8);
  assert.equal(view(latest)[0], TICKS + 7);
  assert.equal(view(held[1])[41], 16);
  assert.equal(ghost[0], first);
  await send({ type: 'pause' });
  await send({ wakes: 1, elapsed: 5000 });
  await send({ type: 'resume' });
  assert.equal((await send({ type: 'exportReplay' }))[0].ticks, TICKS + 8);
  assert.equal((await send({ type: 'loadGhost', replay: '{}' }))[0].code, 'UNSUPPORTED_CAPABILITY');
  assert.equal((await send({ type: 'verifyReplay', json: '{' }))[0].code, 'INVALID_REPLAY');
  await send({
    type: 'newSession',
    profileJson: JSON.stringify({ id: 'p', name: 'P', params: {} }),
    generation: 2,
  });
  assert.equal((await send({ type: 'exportReplay', generation: 2 }))[0].ticks, 0);
});

for (const [fault, command] of [
  ['lab_new', load],
  ['lab_step', { wakes: 1, elapsed: 10 }],
  ['lab_state_ptr', { wakes: 1, elapsed: 10 }],
  ['verify_replay', { type: 'verifyReplay', json: '{}' }],
]) {
  test('terminal WASM trap in ' + fault, async (t) => {
    const send = host(t, fault);
    if (fault !== 'lab_new') {
      await send(load);
      await send({ type: 'start' });
    }
    assert.equal((await send(command)).at(-1)?.type, 'fatal');
    assert.deepEqual(await send({ wakes: 5, elapsed: 100 }), []);
    assert.deepEqual(await send({ type: 'exportReplay' }), []);
  });
}

test('public control rejects bad versions, invalid fields and unsupported future loads', async (t) => {
  for (const invalid of [
    { ...load, protocolVersion: 2 },
    { type: 'input', frame: { steer: 'x' } },
    { type: 'unrecognized' },
  ]) {
    const send = host(t);
    assert.equal((await send(invalid))[0]?.type, 'fatal');
  }
  const send = host(t);
  assert.equal((await send({ ...load, race: { laps: 3 } }))[0].code, 'UNSUPPORTED_CAPABILITY');
  // A rejected load does not advance the generation, so the still-empty worker is at generation 0.
  assert.equal((await send({ type: 'start', generation: 0 }))[0].code, 'SIM_NOT_READY');
  assert.equal(
    (await send({ type: 'hashContent', requestId: 12, bytes: new Uint8Array() }))[0].hash,
    'af1349b9f5f9a1a6a0404dea36dcc949',
  );
});
test('input quantization, saturation and one-tick flags survive the wire', async (t) => {
  const send = host(t);
  await send(load);
  await send({ type: 'start' });
  await send({ type: 'input', frame: { steer: -2, throttle: 2, brake: -1, handbrake: true } });
  await send({ type: 'reset' });
  await send({ type: 'restart' });
  await send({ wakes: 2, elapsed: 9 });
  const replay = JSON.parse((await send({ type: 'exportReplay' }))[0].json);
  assert.deepEqual(replay.inputs, [
    [1, 0x0700ff81],
    [1, 0x0100ff81],
  ]);
  assert.equal(replay.format, 'race-lab-replay/1');
});

test('superseded and disposed async loads cannot publish a stale ready', async (t) => {
  const first = { ...load, generation: 1, protocolVersion: 1 };
  const loadAgain = host(t, undefined, true);
  const replies = await loadAgain({ first, duringInit: { ...first, generation: 2 } });
  assert.deepEqual(
    replies.map((m) => [m.type, m.generation]),
    [
      ['session', 2],
      ['ready', 2],
    ],
  );
  const cancelled = host(t, undefined, true);
  assert.deepEqual(
    await cancelled({ first, duringInit: { type: 'dispose', generation: 1, protocolVersion: 1 } }),
    [],
  );
});

test('a rejected future load leaves the live session running', async (t) => {
  const send = host(t);
  await send(load);
  await send({ type: 'start' });
  assert.equal((await send({ wakes: 1, elapsed: 100 })).at(-1).type, 'snapshot');
  const [rejected] = await send({ ...load, generation: 2, race: { laps: 3 } });
  assert.equal(rejected.code, 'UNSUPPORTED_CAPABILITY');
  const [snapshot] = await send({ wakes: 1, elapsed: 100 });
  assert.equal(snapshot.type, 'snapshot');
  assert.equal(snapshot.generation, 1);
});

test('an invalid profile is a recoverable command error and keeps the session', async (t) => {
  const send = host(t);
  await send(load);
  const [error] = await send({ type: 'newSession', profileJson: '{', generation: 2 });
  assert.deepEqual([error.type, error.code], ['commandError', 'INVALID_PROFILE']);
  assert.equal((await send({ type: 'exportReplay', generation: 2 }))[0].type, 'replay');
});

test('a paused session shows its initial state and pause publishes the final one', async (t) => {
  const send = host(t);
  await send(load);
  const profileJson = JSON.stringify({ id: 'p', name: 'P', params: {} });
  const replies = await send({ type: 'newSession', profileJson, generation: 2 });
  assert.deepEqual(
    replies.map((m) => m.type),
    ['session', 'snapshot'],
  );
  assert.equal(replies[1].paused, true);
  assert.equal(new Float32Array(replies[1].buffer)[41], 0);
  await send({ type: 'returnBuffer', slot: replies[1].slot, buffer: replies[1].buffer, generation: 2 }, [
    replies[1].buffer,
  ]);
  await send({ type: 'start', generation: 2 });
  const [running] = await send({ wakes: 1, elapsed: 20 });
  await send({ type: 'returnBuffer', slot: running.slot, buffer: running.buffer, generation: 2 }, [
    running.buffer,
  ]);
  const [paused] = await send({ type: 'pause', generation: 2 });
  assert.equal(paused.paused, true);
});

test('content hashing does not depend on the current generation', async (t) => {
  const send = host(t);
  await send(load);
  const [reply] = await send({ type: 'hashContent', requestId: 3, bytes: new Uint8Array(), generation: 9 });
  assert.equal(reply.hash, 'af1349b9f5f9a1a6a0404dea36dcc949');
});

test('AR28 dev commands: teleport moves the car, seek re-simulates a replay up to a tick and pauses', async (t) => {
  const { STATE_FIELDS } = await import('../../packages/contracts/dist/index.js');
  const field = (m, name) => new Float32Array(m.buffer)[STATE_FIELDS.length + STATE_FIELDS.indexOf(name)];
  const send = host(t);
  await send(load);
  const [moved] = await send({ type: 'devTeleport', s: 200 });
  assert.equal(moved.type, 'snapshot');
  assert.equal(moved.paused, true);
  assert.ok(Math.abs(field(moved, 'progressS') - 200) < 3, String(field(moved, 'progressS')));
  const golden = readFileSync(new URL('../../crates/sim-core/tests/golden/test-loop.scripted.json', import.meta.url), 'utf8');
  const seek = (tick) => send({ type: 'devSeekReplay', json: golden, tick });
  const first = await seek(600);
  assert.deepEqual(
    first.map((m) => m.type),
    ['session', 'snapshot'],
  );
  assert.equal(field(first[1], 'tick'), 600);
  assert.equal(first[1].paused, true);
  const again = await seek(600);
  assert.deepEqual(new Float32Array(again[1].buffer), new Float32Array(first[1].buffer), 'deterministic seek');
  const ticks = JSON.parse(golden).ticks;
  for (const [json, tick] of [
    [golden, ticks + 1],
    ['{', 10],
  ]) {
    const [rejected] = await send({ type: 'devSeekReplay', json, tick });
    assert.deepEqual([rejected.type, rejected.code], ['verifyError', 'INVALID_REPLAY']);
  }
  // The session survives a rejected seek.
  assert.equal((await send({ type: 'exportReplay' }))[0].type, 'replay');
});
