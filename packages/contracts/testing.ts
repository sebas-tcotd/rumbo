import type { Challenge } from './ports.js';
import { STATE_FIELDS } from './snapshot.js';
import type { StateMsg } from './protocol.js';
import { PROTOCOL_VERSION, type CommandMessage, type SimCommand } from './protocol.js';

export function createSnapshot(overrides: Partial<StateMsg> = {}): StateMsg {
  return {
    type: 'state',
    prev: new Float32Array(STATE_FIELDS.length),
    curr: new Float32Array(STATE_FIELDS.length),
    at: 0,
    stepMs: 0,
    simHz: 120,
    autopilot: false,
    paused: false,
    ...overrides,
  };
}
/** Build a versioned control request without DOM or WASM. */
export function createCommand(command: SimCommand = { type: 'start' }, generation = 1): CommandMessage {
  return { protocolVersion: PROTOCOL_VERSION, generation, ...command };
}
export function createFatal(message = 'Test failure') {
  return { type: 'fatal', code: 'SIM_FAILED', message } as const;
}

/** Prospective D38 fixture. Payload bytes are test inputs, never an emitted challenge format. */
export function createChallenge(overrides: Partial<Challenge> = {}): Challenge {
  return {
    race: new Uint8Array([1]),
    replay: new Uint8Array([2]),
    routeMeta: { name: 'Lima', durationEstimate: 60 },
    ...overrides,
  };
}
