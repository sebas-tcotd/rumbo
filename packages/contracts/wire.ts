import type { CommandMessage, ResponseMessage } from './protocol.js';
import { DEV_SEEK_MAX_CHARS, DEV_SEEK_MAX_TICKS, PROTOCOL_VERSION } from './protocol.js';
import { SNAPSHOT_BYTES, SNAPSHOT_SLOTS, SNAPSHOT_VERSION } from './snapshot.js';

type Check = ((value: unknown) => boolean) & { requiredKeys?: readonly string[] };
const record = (v: unknown): v is Record<string, unknown> =>
  v !== null && typeof v === 'object' && !Array.isArray(v);
const number: Check = (v) => typeof v === 'number' && Number.isFinite(v);
const integer: Check = (v) => typeof v === 'number' && Number.isSafeInteger(v) && v >= 0;
const string: Check = (v) => typeof v === 'string';
const bool: Check = (v) => typeof v === 'boolean';
const nil: Check = (v) => v === null;
function fields(schema: Record<string, Check>): Check {
  const entries = Object.entries(schema);
  const check = (value: unknown) => {
    if (!record(value)) return false;
    for (const [key, accepts] of entries) if (!accepts(value[key])) return false;
    return true;
  };
  return Object.assign(check, { requiredKeys: Object.keys(schema).sort() });
}
const array =
  (check: Check): Check =>
  (v) =>
    Array.isArray(v) && v.every(check);
const tuple =
  (n: number): Check =>
  (v) =>
    Array.isArray(v) && v.length === n && v.every(number);
const f32: Check = (v) => v instanceof Float32Array;
const buffer: Check = (v) => v instanceof ArrayBuffer && v.byteLength === SNAPSHOT_BYTES;
const slot: Check = (v) => typeof v === 'number' && Number.isInteger(v) && v >= 0 && v < SNAPSHOT_SLOTS;
const hashes = fields({ track: string, profile: string });
const mesh = fields({
  positions: f32,
  surfaces: (v) => v instanceof Uint8Array,
  indices: (v) => v instanceof Uint32Array,
});
const input = fields({ steer: number, throttle: number, brake: number, handbrake: bool });
const empty: Check = () => true;
const report = fields({
  ok: bool,
  ticks: integer,
  expectedFinal: string,
  actualFinal: string,
  firstMismatchTick: (v) => v === null || integer(v),
  lapsMatch: bool,
  message: string,
});

const commands = {
  load: fields({
    zone: (v) => v === null || record(v),
    race: (v) => v === null || record(v),
    track: string,
    profile: string,
  }),
  start: empty,
  pause: empty,
  resume: empty,
  dispose: empty,
  reset: empty,
  restart: empty,
  exportReplay: empty,
  input: fields({ frame: input }),
  loadGhost: fields({ replay: string }),
  verifyReplay: fields({ json: string }),
  newSession: fields({ profileJson: string }),
  autopilot: fields({ on: bool }),
  hashContent: fields({ bytes: (v) => v instanceof Uint8Array, requestId: integer }),
  returnBuffer: fields({ slot, buffer }),
  devTeleport: fields({ s: number }),
  devSeekReplay: fields({
    json: (v) => typeof v === 'string' && v.length <= DEV_SEEK_MAX_CHARS,
    tick: (v) => integer(v) && (v as number) <= DEV_SEEK_MAX_TICKS,
  }),
} satisfies Record<CommandMessage['type'], Check>;
const responses = {
  ready: fields({
    layout: array(string),
    schema: array(
      fields({
        key: string,
        group: string,
        label: string,
        unit: string,
        help: string,
        default: number,
        min: number,
        max: number,
        step: number,
      }),
    ),
    meshes: fields({ drivable: mesh, walls: mesh, decals: mesh }),
    centerline: f32,
    barriers: array(fields({ line: array(tuple(3)), arrow: number })),
    groundY: number,
    trackLength: number,
    checkpoints: array(number),
    track: fields({
      id: string,
      name: string,
      closed: bool,
      startS: number,
      finishS: number,
      length: number,
      spacing: number,
    }),
    vehicle: fields({ halfExtents: tuple(3), wheelRadius: number, hardpoints: array(tuple(3)) }),
    hashes,
    physicsVersion: string,
    tickHz: (v) => v === 120,
  }),
  commandError: fields({
    code: (v) => v === 'SIM_NOT_READY' || v === 'INVALID_PROFILE' || v === 'EXPORT_FAILED',
    operation: string,
  }),
  contentHash: fields({ requestId: integer, hash: (v) => typeof v === 'string' && /^[a-f0-9]{32}$/.test(v) }),
  ghostReady: fields({ trackLen: number }),
  snapshot: fields({
    buffer,
    slot,
    layoutVersion: (v) => v === SNAPSHOT_VERSION,
    at: number,
    stepMs: number,
    simHz: number,
    autopilot: bool,
    paused: bool,
  }),
  session: fields({ hashes }),
  lap: fields({ ticks: integer, best: bool, ghost: (v) => nil(v) || f32(v) }),
  replay: fields({ json: string, ticks: integer }),
  verify: fields({ report }),
  verifyError: fields({ code: (v) => v === 'INVALID_REPLAY', message: string }),
  fatal: fields({ code: (v) => v === 'SIM_FAILED', message: string }),
  unsupported: fields({ code: (v) => v === 'UNSUPPORTED_CAPABILITY', operation: string }),
} satisfies Record<ResponseMessage['type'], Check>;

/** Required payload keys are taken from the active validators, in stable lexical order. */
export function protocolDescriptor() {
  const keys = (table: Record<string, Check>) =>
    Object.fromEntries(Object.entries(table).map(([tag, check]) => [tag, check.requiredKeys ?? []]));
  return { commands: keys(commands), responses: keys(responses) };
}

function valid(table: Record<string, Check>, value: unknown) {
  return (
    record(value) &&
    value.protocolVersion === PROTOCOL_VERSION &&
    integer(value.generation) &&
    typeof value.type === 'string' &&
    Object.hasOwn(table, value.type) &&
    table[value.type](value)
  );
}
export function isCommand(value: unknown): value is CommandMessage {
  return valid(commands, value);
}
export function isResponse(value: unknown): value is ResponseMessage {
  return valid(responses, value);
}

/** Prospective header only. A decimal string preserves the full unsigned 64-bit seed on the JSON boundary. */
export const isReplayHeader = fields({
  format: (v) => v === 'rumbo-replay/2',
  physicsVersion: string,
  trackHash: string,
  raceHash: string,
  profileRef: fields({ id: string, hash: string }),
  seed: (v) => typeof v === 'string' && /^(0|[1-9][0-9]{0,19})$/.test(v) && BigInt(v) <= 0xffffffffffffffffn,
});
/** A base64url payload never has length 1 modulo 4. */
export const isLinkHeader: Check = (v) =>
  typeof v === 'string' && /^#v1\.i\.[A-Za-z0-9_-]+$/.test(v) && (v.length - '#v1.i.'.length) % 4 !== 1;
