import type { SettingsStorePort } from '@rumbo/contracts';
import { createKeyboardGamepadInput, type KeyboardGamepadInput } from './input/index.ts';
import { createLocalSettingsStore, type KeyValueStorage } from './storage/index.ts';
import { createVisibilitySource, type VisibilitySource } from './visibility.ts';

/** Browser services the composition root hands to the other layers. */
export interface WebPlatform {
  input: KeyboardGamepadInput;
  settings: SettingsStorePort;
  visibility: VisibilitySource;
  /** Removes every listener the platform installed. */
  dispose(): void;
}

/** What the browser platform needs from its host. */
export interface WebPlatformOptions {
  window: Window;
  /** Usually `safeLocalStorage()`; null when storage is unavailable. */
  storage: KeyValueStorage | null;
  /** Lab key map: called with `KeyboardEvent.code` on each press and on gamepad action edges. */
  onKeyAction: (code: string) => void;
}

/** Aggregates the platform adapters that exist today (input, settings, visibility). */
export function createWebPlatform(options: WebPlatformOptions): WebPlatform {
  const input = createKeyboardGamepadInput(options.window, options.onKeyAction);
  return {
    input,
    settings: createLocalSettingsStore(options.storage),
    visibility: createVisibilitySource(options.window.document),
    dispose: () => input.dispose(),
  };
}
