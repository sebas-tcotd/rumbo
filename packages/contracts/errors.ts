/** A successful value or a failure with a stable code. */
export type Result<T, E extends { code: string }> = { ok: true; value: T } | { ok: false; error: E };

/** Construct a successful result. */
export function ok<T>(value: T): { ok: true; value: T } {
  return { ok: true, value };
}

/** Construct a failed result. */
export function err<const E extends { code: string }>(error: E): { ok: false; error: E } {
  return { ok: false, error };
}

/** Impact on the current session or attempt. */
export type ErrorLevel = 'fatal' | 'recoverable' | 'degraded' | 'invalidAttempt';

/** Stable UI errors and their impact. Each code has a Spanish translation. */
export const ERROR_CATALOG = {
  BROWSER_UNSUPPORTED: 'fatal',
  WASM_LOAD_FAILED: 'fatal',
  RENDER_INIT_FAILED: 'fatal',
  SIM_FAILED: 'fatal',
  SIM_NOT_READY: 'recoverable',
  INVALID_TRACK: 'recoverable',
  INVALID_PROFILE: 'recoverable',
  INVALID_REPLAY: 'invalidAttempt',
  EXPORT_FAILED: 'recoverable',
  SCHEMA_FAILED: 'recoverable',
  UNSUPPORTED_CAPABILITY: 'recoverable',
  STORAGE_UNAVAILABLE: 'recoverable',
  GAMEPAD_UNAVAILABLE: 'degraded',
  WEBGPU_UNAVAILABLE: 'degraded',
} as const satisfies Record<string, ErrorLevel>;

/** Code that can be translated by the UI. */
export type ErrorCode = keyof typeof ERROR_CATALOG;

/** Required browser features in display order. */
export const REQUIRED_CAPABILITIES = ['webgl2', 'wasm', 'compressionStream', 'moduleWorker', 'indexedDb'] as const;

/** A browser feature the game needs to boot. */
export type RequiredCapability = (typeof REQUIRED_CAPABILITIES)[number];

/** Errors that require restarting the app. */
export type FatalCode = {
  [Code in ErrorCode]: (typeof ERROR_CATALOG)[Code] extends 'fatal' ? Code : never;
}[ErrorCode];

/** Stable error codes returned by the Rust WASM boundary, in parity order. */
export const SIM_EDGE_ERROR_CODES = [
  'INVALID_TRACK',
  'INVALID_PROFILE',
  'INVALID_REPLAY',
  'EXPORT_FAILED',
  'SCHEMA_FAILED',
] as const satisfies readonly ErrorCode[];

/** Stable errors on the control boundary. */
export type SimErrorCode =
  | 'SIM_NOT_READY'
  | 'INVALID_PROFILE'
  | 'EXPORT_FAILED'
  | 'INVALID_REPLAY'
  | 'UNSUPPORTED_CAPABILITY'
  | 'SIM_FAILED';
