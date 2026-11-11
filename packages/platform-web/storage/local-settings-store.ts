import { DEFAULT_SETTINGS, type SaveResult, type Settings, type SettingsStorePort } from '@rumbo/contracts';
import { type MigratedSettings, migrateSettings, SETTINGS_KEY, serializeSettings } from './settings-schema.ts';

/** The subset of Web Storage the store uses (injectable for tests). */
export type KeyValueStorage = Pick<Storage, 'getItem' | 'setItem'>;

/**
 * SettingsStorePort over Web Storage. Never throws: an inaccessible storage (private mode, quota,
 * disabled) or a corrupt value leaves the app on defaults. Data written by a newer build is read as
 * defaults and is never overwritten by this build.
 */
export function createLocalSettingsStore(
  storage: KeyValueStorage | null,
  migrate: (raw: string | null) => MigratedSettings = migrateSettings,
): SettingsStorePort {
  /** Whether this build may write; decided by the first read (load, or save before any load). */
  let writable: boolean | null = null;
  const read = (): string | null => {
    try {
      return storage?.getItem(SETTINGS_KEY) ?? null;
    } catch {
      return null;
    }
  };
  const loadOnce = (): Settings => {
    const { settings, persist } = safeMigrate(migrate, read());
    writable = persist;
    return settings;
  };
  return {
    load: loadOnce,
    save(next: Settings): SaveResult {
      if (writable === null) loadOnce();
      if (!writable || !storage) return { ok: false, code: 'STORAGE_UNAVAILABLE' };
      try {
        storage.setItem(SETTINGS_KEY, serializeSettings(next));
        return { ok: true };
      } catch {
        return { ok: false, code: 'STORAGE_UNAVAILABLE' };
      }
    },
  };
}

/**
 * `migrateSettings` that never throws: a migration step that fails on unexpected stored data leaves the
 * app on defaults and does not overwrite what is stored (it may be readable by a fixed build).
 */
function safeMigrate(migrate: (raw: string | null) => MigratedSettings, raw: string | null): MigratedSettings {
  try {
    return migrate(raw);
  } catch {
    return { settings: { ...DEFAULT_SETTINGS }, persist: false };
  }
}

/** `window.localStorage`, or null when merely touching it throws (sandboxed iframes, some browsers). */
export function safeLocalStorage(): KeyValueStorage | null {
  try {
    return globalThis.localStorage ?? null;
  } catch {
    return null;
  }
}
