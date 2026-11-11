export const STATE_FIELDS = [
  'tick',
  'px',
  'py',
  'pz',
  'qx',
  'qy',
  'qz',
  'qw',
  'vx',
  'vy',
  'vz',
  'speedKmh',
  'slipDeg',
  'steerDeg',
  'steerInput',
  'drift',
  'contactFL',
  'contactFR',
  'contactRL',
  'contactRR',
  'compFL',
  'compFR',
  'compRL',
  'compRR',
  'surfFL',
  'surfFR',
  'surfRL',
  'surfRR',
  'lap',
  'lapTicks',
  'lastLapTicks',
  'bestLapTicks',
  'nextCheckpoint',
  'progressS',
  'trackLength',
  'yawRate',
  'latG',
  'throttle',
  'brake',
  'invalid',
  'sampleIndex',
] as const;

export type StateField = (typeof STATE_FIELDS)[number];
export type StateIndex = Record<StateField, number>;

/** Validate the complete ordered float32 state layout before constructing views. */
export function buildIndex(layout: readonly string[]): StateIndex {
  if (layout.length !== STATE_FIELDS.length || STATE_FIELDS.some((field, i) => layout[i] !== field)) {
    throw new Error('INCOMPATIBLE_STATE_LAYOUT');
  }
  return Object.fromEntries(layout.map((name, index) => [name, index])) as StateIndex;
}
/** Transport layout version of a snapshot buffer. Bump it when the byte layout below changes. */
export const SNAPSHOT_VERSION = 1;
/** Bytes of one state: STATE_FIELDS.length little-endian f32 values, field i at offset 4 * i. */
export const STATE_BYTES = STATE_FIELDS.length * Float32Array.BYTES_PER_ELEMENT;
/**
 * Bytes of one transported snapshot: the previous state at offset 0 followed by the current state at
 * offset STATE_BYTES, so the host can interpolate between two consecutive ticks.
 */
export const SNAPSHOT_BYTES = 2 * STATE_BYTES;
/**
 * Transport slots (ArrayBuffer) in the worker pool. Ownership rules:
 * - The worker owns a slot until it transfers it inside a `snapshot` message; its view is then detached.
 * - The host owns the slot from receipt until it transfers it back with `returnBuffer`; the host must copy
 *   what it needs before returning it. No view of a returned buffer may be read.
 * - With no free slot the worker keeps stepping, skips the visual publication and publishes the latest
 *   state as soon as a slot returns. Control events (laps, replay, errors) never depend on a slot.
 * - Buffers are never SharedArrayBuffer and are never backed by WASM linear memory.
 */
export const SNAPSHOT_SLOTS = 3;

/** Validate Rust's byte-level layout before borrowing its linear memory. */
export function validateSnapshotDescriptor(value: unknown): void {
  if (
    typeof value !== 'object' ||
    value === null ||
    !('version' in value) ||
    value.version !== SNAPSHOT_VERSION ||
    !('stride' in value) ||
    value.stride !== STATE_BYTES ||
    !('fields' in value) ||
    !Array.isArray(value.fields) ||
    value.fields.length !== STATE_FIELDS.length ||
    !value.fields.every(
      (field: unknown, index) =>
        typeof field === 'object' &&
        field !== null &&
        'name' in field &&
        field.name === STATE_FIELDS[index] &&
        'type' in field &&
        field.type === 'f32' &&
        'offset' in field &&
        field.offset === index * Float32Array.BYTES_PER_ELEMENT,
    )
  ) {
    throw new Error('INCOMPATIBLE_STATE_LAYOUT');
  }
}
