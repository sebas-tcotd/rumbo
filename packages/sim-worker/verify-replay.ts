import type { VerifyReport } from '@rumbo/contracts';
import { parseEdgeError } from './edge-error.js';

type Verification =
  | { type: 'verify'; report: VerifyReport }
  | { type: 'verifyError'; code: 'INVALID_REPLAY'; message: string };

/**
 * Verify `rumbo-replay/2` bytes with the Rust binding. The replay does not carry the vehicle profile
 * (D17): `profile` is the JSON the session runs with. Only domain errors (thrown strings) are recovered.
 */
export function verifyReplay(
  binding: (track: string, replay: Uint8Array, profile: string) => string,
  track: string,
  replay: Uint8Array,
  profile: string,
): Verification {
  let result: string;
  try {
    result = binding(track, replay, profile);
  } catch (cause) {
    // sim-wasm maps Rust Result::Err to a primitive JS string via JsValue::from_str.
    if (typeof cause !== 'string') throw cause;
    return { type: 'verifyError', code: 'INVALID_REPLAY', message: parseEdgeError(cause)?.detail ?? cause };
  }
  return { type: 'verify', report: JSON.parse(result) as VerifyReport };
}
