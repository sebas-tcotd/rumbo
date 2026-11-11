// Mensajes entre el hilo principal y el worker de simulación.
// El worker es dueño del reloj: el hilo principal solo envía la entrada más reciente.

export interface ParamMeta {
  key: string;
  group: string;
  label: string;
  unit: string;
  help: string;
  default: number;
  min: number;
  max: number;
  step: number;
}

export type ProfileParams = Record<string, number>;

export interface ProfileFile {
  id: string;
  name: string;
  params: ProfileParams;
}

export interface MeshData {
  positions: Float32Array;
  surfaces: Uint8Array;
  indices: Uint32Array;
}

export interface VehicleGeometry {
  halfExtents: [number, number, number];
  wheelRadius: number;
  hardpoints: [number, number, number][];
}

export interface VerifyReport {
  ok: boolean;
  ticks: number;
  expectedFinal: string;
  actualFinal: string;
  firstMismatchTick: number | null;
  lapsMatch: boolean;
  message: string;
}

export interface TrackInfo {
  id: string;
  name: string;
  closed: boolean;
  /** Sprint: líneas de salida y llegada sobre `s` (0 en circuitos). */
  startS: number;
  finishS: number;
  length: number;
  spacing: number;
}

export interface Hashes {
  track: string;
  profile: string;
}

export type ToWorker =
  | { type: 'init'; trackJson: string; profileJson: string }
  | { type: 'input'; steer: number; throttle: number; brake: number; handbrake: boolean }
  | { type: 'reset' }
  | { type: 'restart' }
  | { type: 'newSession'; profileJson: string }
  | { type: 'autopilot'; on: boolean }
  | { type: 'pause'; on: boolean }
  | { type: 'exportReplay' }
  | { type: 'verifyReplay'; json: string };

export interface ReadyMsg {
  type: 'ready';
  layout: string[];
  schema: ParamMeta[];
  meshes: { drivable: MeshData; walls: MeshData; decals: MeshData };
  centerline: Float32Array;
  /** Barreras de carrera (calles laterales cerradas): puntos a nivel de calzada y sentido de las flechas. */
  barriers: { line: [number, number, number][]; arrow: number }[];
  groundY: number;
  trackLength: number;
  checkpoints: number[];
  track: TrackInfo;
  vehicle: VehicleGeometry;
  hashes: Hashes;
  physicsVersion: string;
  tickHz: number;
}

export interface StateMsg {
  type: 'state';
  prev: Float32Array;
  curr: Float32Array;
  /** Hora absoluta (timeOrigin + now) en que debía ocurrir `curr`. */
  at: number;
  stepMs: number;
  simHz: number;
  autopilot: boolean;
  paused: boolean;
}

export interface LapMsg {
  type: 'lap';
  ticks: number;
  best: boolean;
  /** Pose por tick de la vuelta: GHOST_STRIDE flotantes por frame. Solo si es mejor vuelta. */
  ghost: Float32Array | null;
}

export type FromWorker =
  | ReadyMsg
  | StateMsg
  | LapMsg
  | { type: 'session'; hashes: Hashes }
  | { type: 'replay'; json: string; ticks: number }
  | { type: 'verify'; report: VerifyReport }
  | { type: 'verifyError'; code: 'INVALID_REPLAY'; message: string }
  | { type: 'fatal'; code: 'SIM_FAILED'; message: string };

/** px, py, pz, qx, qy, qz, qw, s */
export const GHOST_STRIDE = 8;

export const FLAG_HANDBRAKE = 1;
export const FLAG_RESET = 2;
export const FLAG_RESTART = 4;

export const PROTOCOL_VERSION = 1;
export type InputFrame = Omit<Extract<ToWorker, { type: 'input' }>, 'type'>;
export type SimCommand =
  | Exclude<ToWorker, { type: 'init' | 'input' | 'pause' }>
  | {
      type: 'load';
      zone: Record<string, unknown> | null;
      race: Record<string, unknown> | null;
      track: string;
      profile: string;
    }
  | { type: 'input'; frame: InputFrame }
  | { type: 'start' | 'pause' | 'resume' | 'dispose' }
  | { type: 'loadGhost'; replay: string };
export type SimResponse =
  | FromWorker
  | { type: 'commandError'; code: 'SIM_NOT_READY' | 'INVALID_PROFILE' | 'EXPORT_FAILED'; operation: string }
  | { type: 'ghostReady'; trackLen: number }
  | { type: 'unsupported'; code: 'UNSUPPORTED_CAPABILITY'; operation: string };
export type SnapshotMessage = Omit<StateMsg, 'type' | 'prev' | 'curr'> & {
  type: 'snapshot';
  slot: number;
  buffer: ArrayBuffer;
  layoutVersion: number;
};
export interface Envelope {
  protocolVersion: number;
  generation: number;
}
export type HashRequest = { type: 'hashContent'; requestId: number; bytes: Uint8Array };
export type HashReply = { type: 'contentHash'; requestId: number; hash: string };
export type CommandMessage = Envelope &
  (SimCommand | HashRequest | { type: 'returnBuffer'; slot: number; buffer: ArrayBuffer });
export type ResponseMessage = Envelope & (Exclude<SimResponse, StateMsg> | HashReply | SnapshotMessage);
