import type { VerifyReport } from '@rumbo/contracts';
import { parseEdgeError } from './edge-error.js';

type Verification =
  | { type: 'verify'; report: VerifyReport }
  | { type: 'verifyError'; code: 'INVALID_REPLAY'; message: string };

/** Recover only domain errors returned by the Rust verification binding. */
export function verifyReplay(
  binding: (track: string, replay: string) => string,
  track: string,
  replay: string,
): Verification {
  let result: string;
  try {
    result = binding(track, replay);
  } catch (cause) {
    // sim-wasm maps Rust Result::Err to a primitive JS string via JsValue::from_str.
    if (typeof cause !== 'string') throw cause;
    return { type: 'verifyError', code: 'INVALID_REPLAY', message: parseEdgeError(cause)?.detail ?? cause };
  }
  return { type: 'verify', report: JSON.parse(result) as VerifyReport };
}
