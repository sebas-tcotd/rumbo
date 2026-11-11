import type { InputFrame, StateMsg, SimCommand } from './protocol.js';

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
export interface RendererPort {
  setState(snapshot: StateMsg): void;
}
export interface InputSourcePort {
  sample(): InputFrame;
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
