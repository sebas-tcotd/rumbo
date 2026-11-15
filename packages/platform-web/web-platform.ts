import type { LogPort, SettingsStorePort } from '@rumbo/contracts';
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
  /** Optional session logger for platform degradation. */
  logger?: LogPort;
  /** Optional failure detected before the input adapter was created. */
  gamepadFailure?: 'missing' | 'blocked';
}

/** Aggregates the platform adapters that exist today (input, settings, visibility). */
export function createWebPlatform(options: WebPlatformOptions): WebPlatform {
  let reported = false;
  const reportGamepadFailure = (reason: NonNullable<WebPlatformOptions['gamepadFailure']>) => {
    if (reported) return;
    reported = true;
    queueMicrotask(() => options.logger?.log('warn', 'input', 'GAMEPAD_UNAVAILABLE', { reason }));
  };
  if (options.gamepadFailure) reportGamepadFailure(options.gamepadFailure);
  const input = createKeyboardGamepadInput(options.window, options.onKeyAction, reportGamepadFailure);
  return {
    input,
    settings: createLocalSettingsStore(options.storage),
    visibility: createVisibilitySource(options.window.document),
    dispose: () => input.dispose(),
  };
}
