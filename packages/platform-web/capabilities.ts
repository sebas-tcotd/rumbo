import { REQUIRED_CAPABILITIES, type RequiredCapability } from '@rumbo/contracts';

/** Results of independent browser feature probes. */
export interface CapabilityReport {
  required: Record<RequiredCapability, boolean>;
  optional: { gamepad: boolean; webgpu: boolean };
  gamepadFailure?: 'missing' | 'blocked';
}

const EMPTY_WASM = new Uint8Array([0, 97, 115, 109, 1, 0, 0, 0]);

function probe(check: () => boolean): boolean {
  try {
    return check();
  } catch {
    return false;
  }
}

function webgl2(target: Window): boolean {
  const gl = target.document.createElement('canvas').getContext('webgl2');
  if (!gl) return false;
  gl.getExtension('WEBGL_lose_context')?.loseContext();
  return true;
}

function wasm(target: typeof globalThis): boolean {
  if (!target.WebAssembly?.validate(EMPTY_WASM)) return false;
  // Validation alone does not detect a CSP that disallows compiling WASM.
  new target.WebAssembly.Module(EMPTY_WASM);
  return true;
}

function compression(target: typeof globalThis): boolean {
  new target.CompressionStream('deflate-raw');
  new target.DecompressionStream('deflate-raw');
  return true;
}

function moduleWorker(target: typeof globalThis): boolean {
  let read = false;
  try {
    // WebIDL reads the option before parsing this deliberately invalid URL.
    new target.Worker('http://[', { get type(): WorkerType { read = true; return 'module'; } });
  } catch {
    return read;
  }
  return false;
}

function gamepadFailure(target: Window): CapabilityReport['gamepadFailure'] {
  try {
    return typeof target.navigator.getGamepads === 'function' && target.navigator.getGamepads() ? undefined : 'missing';
  } catch {
    return 'blocked';
  }
}

/** Check browser features without loading game code or creating a worker. */
export function probeCapabilities(target: Window & typeof globalThis): CapabilityReport {
  const failure = gamepadFailure(target);
  return {
    required: {
      webgl2: probe(() => webgl2(target)),
      wasm: probe(() => wasm(target)),
      compressionStream: probe(() => compression(target)),
      moduleWorker: moduleWorker(target),
      indexedDb: probe(() => typeof target.indexedDB?.open === 'function'),
    },
    optional: {
      gamepad: failure === undefined,
      webgpu: probe(() => 'gpu' in target.navigator),
    },
    gamepadFailure: failure,
  };
}

/** Missing startup features, in display order. */
export function missingRequired(report: CapabilityReport): RequiredCapability[] {
  return REQUIRED_CAPABILITIES.filter((name) => !report.required[name]);
}
