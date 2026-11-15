import { SIM_EDGE_ERROR_CODES } from '@rumbo/contracts';

/** Read the stable prefix of a WASM rejection. Traps are objects and never match. */
export function parseEdgeError(cause: unknown) {
  if (typeof cause !== 'string') return null;
  const separator = cause.indexOf(': ');
  if (separator < 0) return null;
  const code = SIM_EDGE_ERROR_CODES.find((entry) => entry === cause.slice(0, separator));
  return code ? { code, detail: cause.slice(separator + 2) } : null;
}
