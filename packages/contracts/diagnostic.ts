import { ERROR_CATALOG, REQUIRED_CAPABILITIES, type ErrorCode } from './errors.js';
import type { CounterSample, LogChannel, LogRecord } from './logging.js';

/** Data explicitly gathered by the host for a player-initiated copy. */
export interface DiagnosticInput {
  build: string;
  copiedAt: string;
  code: ErrorCode;
  channel: LogChannel;
  detail?: string;
  physicsVersion?: string;
  protocolVersion?: number;
  backend: string;
  browser: string;
  capabilities: { required: Record<string, boolean>; optional: Record<string, boolean> };
  log: readonly LogRecord[];
  counters: readonly CounterSample[];
}

// Runtime messages can contain arbitrary player or machine data. Export only known literals.
const safeDetails = new Set([
  'Failed to fetch', 'Load failed', 'NetworkError when attempting to fetch resource.',
  'unreachable', 'memory access out of bounds', 'WORKER_FAILED', 'INVALID_WORKER_MESSAGE',
  'INVALID_RESPONSE', 'SIM_FAILED', 'WASM_LOAD_FAILED',
]);

function safeDetail(detail: string): string {
  return safeDetails.has(detail) ? detail : '<redacted>';
}

function browserVersion(userAgent: string): string {
  const product = /\b(?:Edg|EdgiOS|EdgA|OPR)\/[\d.]+\b/.exec(userAgent)?.[0]
    ?? /\b(?:Chrome|Chromium|CriOS|Firefox|FxiOS)\/[\d.]+\b/.exec(userAgent)?.[0];
  if (product) return product;
  const safariVersion = /\bVersion\/([\d.]+) Safari\//.exec(userAgent)?.[1];
  return safariVersion ? 'Safari/' + safariVersion : 'unknown';
}

function logLine({ t, level, channel, code, fields }: LogRecord): string {
  const values: string[] = [];
  if (typeof fields.detail === 'string') values.push(' detail=' + safeDetail(fields.detail));
  if (code === 'GAMEPAD_UNAVAILABLE' && (fields.reason === 'missing' || fields.reason === 'blocked')) {
    values.push(' reason=' + fields.reason);
  }
  if (code === 'REQUIRED_CAPABILITY_MISSING') {
    const capability = REQUIRED_CAPABILITIES.find((name) => name === fields.name);
    if (capability) values.push(' name=' + capability);
  }
  return '  ' + t.toFixed(3) + ' ' + level.toUpperCase() + ' ' + channel + ' ' + code + values.join('');
}

/** Format a diagnostic as plain text using only approved metadata and technical messages. */
export function formatDiagnostic(input: DiagnosticInput): string {
  const required = REQUIRED_CAPABILITIES.filter((name) => Object.hasOwn(input.capabilities.required, name));
  const optional = (['gamepad', 'webgpu'] as const).filter((name) => Object.hasOwn(input.capabilities.optional, name));
  const capabilities = [...required.map((name) => [name, input.capabilities.required[name]]),
    ...optional.map((name) => [name, input.capabilities.optional[name]])];
  const physics = /^\d+\.\d+\.\d+$/.exec(input.physicsVersion ?? '')?.[0] ?? 'unknown';
  const backend = input.backend === 'WebGPU' || input.backend === 'WebGL 2' ? input.backend : 'none';
  const copiedAt = /^\d{4}-\d\d-\d\dT\d\d:\d\d:\d\d\.\d{3}Z$/.exec(input.copiedAt)?.[0] ?? 'unknown';
  return [
    'Rumbo diagnostic',
    'build: ' + (/^(?:[\da-f]{6,40}|dev)$/.exec(input.build)?.[0] ?? 'unknown') + '   copied: ' + copiedAt,
    'error: ' + input.code + ' (' + ERROR_CATALOG[input.code] + ', channel ' + input.channel + ')',
    ...(input.detail ? ['detail: ' + safeDetail(input.detail)] : []),
    'physics: ' + physics + '   protocol: ' + (input.protocolVersion ?? 'unknown') + '   backend: ' + backend,
    'capabilities: ' + capabilities.map(([key, enabled]) => key + '=' + (enabled ? 'yes' : 'no')).join(' '),
    'browser: ' + browserVersion(input.browser),
    'counters (last 60 s): t simMs simHz frameMs',
    ...input.counters.map(({ t, simMs, simHz, frameMs }) => '  ' + [t, simMs, simHz, frameMs].join(' ')),
    'log (last 256):',
    ...input.log.map(logLine),
  ].join('\n');
}
