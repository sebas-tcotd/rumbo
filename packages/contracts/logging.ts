/** Console severity in ascending order. */
export type LogLevel = 'trace' | 'debug' | 'info' | 'warn' | 'error';

/** Stable architecture channels. */
export const LOG_CHANNELS = ['sim', 'compiler', 'render', 'audio', 'input', 'storage', 'game', 'net'] as const;

/** Subsystem that emitted a record. */
export type LogChannel = (typeof LOG_CHANNELS)[number];

/** Flat, serializable fields retained by the logger. */
export type LogFields = Record<string, string | number | boolean | null>;

/** A log entry timestamped with the host monotonic clock. */
export type LogRecord = { t: number; level: LogLevel; channel: LogChannel; code: string; fields: LogFields };

/** Host-injected logging boundary. */
export interface LogPort {
  log(level: LogLevel, channel: LogChannel, code: string, fields?: Record<string, unknown>): void;
}

type Sink = Record<Exclude<LogLevel, 'trace'>, (message: string, fields: LogFields) => void>;
type LoggerOptions = { capacity: number; consoleLevel: LogLevel; sink: Sink; now: () => number };
const priority = { trace: 0, debug: 1, info: 2, warn: 3, error: 4 } as const;

function createRing<T>(capacity: number) {
  if (!Number.isSafeInteger(capacity) || capacity < 1) throw new TypeError('Invalid ring capacity');
  const entries = new Array<T>(capacity);
  let next = 0;
  let size = 0;
  return {
    push(entry: T) {
      entries[next] = entry;
      next = (next + 1) % capacity;
      size = Math.min(size + 1, capacity);
    },
    read() {
      return size < capacity ? entries.slice(0, size) : entries.slice(next).concat(entries.slice(0, next));
    },
  };
}

function flatten(value: unknown): LogFields[string] {
  if (value === null || typeof value === 'string' || typeof value === 'number' || typeof value === 'boolean')
    return value;
  try {
    return value instanceof Error ? value.message : String(value);
  } catch {
    return '<unavailable>';
  }
}

/** Create a session logger with a bounded history and an injected console sink. */
export function createLogger({ capacity, consoleLevel, sink, now }: LoggerOptions): LogPort & { records(): LogRecord[] } {
  const ring = createRing<LogRecord>(capacity);
  return {
    log(level, channel, code, fields = {}) {
      if (!LOG_CHANNELS.includes(channel) || !/^[A-Z][A-Z0-9_]*$/.test(code))
        throw new TypeError('Invalid log channel or code');
      if (level === 'trace' && priority[consoleLevel] > priority.debug) return;
      const flat = Object.fromEntries(Object.entries(fields).map(([key, value]) => [key, flatten(value)]));
      ring.push({ t: now(), level, channel, code, fields: flat });
      const method = level === 'trace' ? 'debug' : level;
      if (priority[method] >= priority[consoleLevel]) sink[method]('[' + channel + '] ' + code, flat);
    },
    records: () => ring.read().map((entry) => ({ ...entry, fields: { ...entry.fields } })),
  };
}

/** A periodic performance sample from the host. */
export type CounterSample = { t: number; simMs: number; simHz: number; frameMs: number };

/** Bounded counter history in chronological order. */
export function createCounterRing({ capacity }: { capacity: number }) {
  const ring = createRing<CounterSample>(capacity);
  return {
    push: (sample: CounterSample) => ring.push({ ...sample }),
    samples: () => ring.read().map((sample) => ({ ...sample })),
  };
}
