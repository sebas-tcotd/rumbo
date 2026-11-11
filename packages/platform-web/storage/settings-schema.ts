import { DEFAULT_SETTINGS, type Settings, SETTINGS_VERSION, SUPPORTED_LOCALES } from '@rumbo/contracts';

/** localStorage key of the player settings (AR19). Fixed: the version lives inside the value. */
export const SETTINGS_KEY = 'rumbo.settings.v1';

/** Stored record of any version: `{ version, ...fields }`. */
export type StoredSettings = { version: number } & Record<string, unknown>;

/**
 * Pure step from version N to N+1. Every story that adds a setting appends one migration to
 * MIGRATIONS, bumps SETTINGS_VERSION in @rumbo/contracts and extends `Settings` and `sanitize`.
 */
export type SettingsMigration = (from: StoredSettings) => StoredSettings;

/** v1 is the first schema: nothing to migrate yet. MIGRATIONS[n - 1] turns vN into vN+1. */
export const MIGRATIONS: readonly SettingsMigration[] = [];

/** Result of reading stored settings. `persist` is false when the data came from a newer build. */
export interface MigratedSettings {
  settings: Settings;
  persist: boolean;
}

const isRecord = (v: unknown): v is Record<string, unknown> =>
  typeof v === 'object' && v !== null && !Array.isArray(v);

/** Keeps each valid field and replaces invalid ones (wrong type, out of range) with its default. */
export function sanitize(raw: Record<string, unknown>): Settings {
  const locale = SUPPORTED_LOCALES.find((l) => l === raw.locale) ?? DEFAULT_SETTINGS.locale;
  return { locale };
}

function parse(raw: string | null): unknown {
  if (raw === null) return null;
  try {
    return JSON.parse(raw);
  } catch {
    return null;
  }
}

function storedVersion(data: unknown): StoredSettings | null {
  if (!isRecord(data)) return null;
  const { version } = data;
  if (typeof version !== 'number' || !Number.isSafeInteger(version) || version < 1) return null;
  return data as StoredSettings;
}

/**
 * Turns the stored string into current settings. Pure: invalid JSON or a missing/non-integer version
 * yield defaults; an older version runs the migration chain (`current` is injectable for tests); a
 * newer version yields defaults in memory and must not overwrite storage (`persist: false`).
 */
export function migrateSettings(
  raw: string | null,
  migrations: readonly SettingsMigration[] = MIGRATIONS,
  current = SETTINGS_VERSION,
): MigratedSettings {
  const found = storedVersion(parse(raw));
  if (!found) return { settings: { ...DEFAULT_SETTINGS }, persist: true };
  let stored: StoredSettings = found;
  if (stored.version > current) return { settings: { ...DEFAULT_SETTINGS }, persist: false };
  while (stored.version < current) {
    const step = migrations[stored.version - 1];
    if (!step) return { settings: { ...DEFAULT_SETTINGS }, persist: true };
    stored = { ...step(stored), version: stored.version + 1 };
  }
  return { settings: sanitize(stored), persist: true };
}

/** Serialises current settings with their version. */
export function serializeSettings(settings: Settings, version = SETTINGS_VERSION): string {
  return JSON.stringify({ version, ...sanitize({ ...settings }) });
}
