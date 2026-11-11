import type { SaveResult, Settings, SettingsStorePort } from '@rumbo/contracts';
import { migrateSettings, SETTINGS_KEY, serializeSettings } from './settings-schema.ts';

/** The subset of Web Storage the store uses (injectable for tests). */
export type KeyValueStorage = Pick<Storage, 'getItem' | 'setItem'>;

/**
 * SettingsStorePort over Web Storage. Never throws: an inaccessible storage (private mode, quota,
 * disabled) or a corrupt value leaves the app on defaults. Data written by a newer build is read as
 * defaults and is never overwritten by this build.
 */
export function createLocalSettingsStore(storage: KeyValueStorage | null): SettingsStorePort {
  let readOnly = false;
  const read = (): string | null => {
    try {
      return storage?.getItem(SETTINGS_KEY) ?? null;
    } catch {
      return null;
    }
  };
  return {
    load(): Settings {
      const { settings, persist } = migrateSettings(read());
      readOnly = !persist;
      return settings;
    },
    save(next: Settings): SaveResult {
      if (readOnly || !storage) return { ok: false, code: 'STORAGE_UNAVAILABLE' };
      try {
        storage.setItem(SETTINGS_KEY, serializeSettings(next));
        return { ok: true };
      } catch {
        return { ok: false, code: 'STORAGE_UNAVAILABLE' };
      }
    },
  };
}

/** `window.localStorage`, or null when merely touching it throws (sandboxed iframes, some browsers). */
export function safeLocalStorage(): KeyValueStorage | null {
  try {
    return globalThis.localStorage ?? null;
  } catch {
    return null;
  }
}
