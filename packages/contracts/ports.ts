import type { InputFrame, ReadyMsg, SimCommand } from './protocol.js';
import type { StateIndex } from './snapshot.js';
import type { Settings } from './settings.js';

/** Commands are scoped to one worker instance. Disposal terminates its pending requests. */
export interface SimPort {
  send(command: SimCommand): void;
  dispose(): void;
}
export interface RouteMeta {
  name: string;
  durationEstimate: number;
}
/** Canonical bytes for each domain artifact come from track-format.
 * The challenge envelope carries display metadata independently of route identity.
 * No challenge payload writer exists until the Race and replay v2 domain formats ship.
 */
export interface Challenge {
  race: Uint8Array;
  replay: Uint8Array;
  routeMeta: RouteMeta;
}
export type CapabilityResult<T> = { ok: true; value: T } | { ok: false; code: 'UNSUPPORTED_CAPABILITY' };
/** Byte encoding and content identity are owned by the Rust adapter. */
export interface LinkCodecPort {
  encodeChallenge(challenge: Challenge): Promise<CapabilityResult<Uint8Array>>;
  decodeChallenge(bytes: Uint8Array): Promise<CapabilityResult<Challenge>>;
  hashContent(bytes: Uint8Array): Promise<string>;
}
/** Body adapter. Hot path: no allocation, no Result, no React. */
export interface RendererPort {
  /** Builds the scene once per ready (track, meshes, vehicle). */
  build(ready: ReadyMsg, index: StateIndex): void;
  /**
   * Draws one display frame between two consecutive ticks. prev/curr are host-owned scratch valid only
   * during the call: the renderer copies what it needs and keeps no reference. 0 ≤ alpha ≤ 1.
   */
  frame(prev: Float32Array, curr: Float32Array, alpha: number): void;
  /** Best-lap ghost poses (GHOST_STRIDE floats per tick), or null to hide it. */
  setGhost(frames: Float32Array | null): void;
  dispose(): void;
}
/** Polled once per display frame by the host loop. */
export interface InputSourcePort {
  sample(): InputFrame;
  /** Removes every listener the source installed. */
  dispose(): void;
}
/** Outcome of a write that may fail without breaking the app. */
export type SaveResult = { ok: true } | { ok: false; code: 'STORAGE_UNAVAILABLE' };
/**
 * Player settings persistence (AR19). The UI reads and saves settings only through this port, never
 * through localStorage. load() never throws: missing, corrupt or unreadable data yields defaults.
 */
export interface SettingsStorePort {
  load(): Settings;
  save(next: Settings): SaveResult;
}
export interface AudioPort {
  setVolume(volume: number): void;
}
export interface RecordsStorePort {
  read(key: string): Promise<Uint8Array | null>;
  write(key: string, bytes: Uint8Array): Promise<void>;
}
export interface TelemetryPort {
  record(event: string): void;
}
export interface ZonePackSourcePort {
  load(hash: string): Promise<Uint8Array>;
}
