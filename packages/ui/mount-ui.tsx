import type { SettingsStorePort } from '@rumbo/contracts';
import { StrictMode } from 'react';
import { createRoot } from 'react-dom/client';
import { I18nextProvider } from 'react-i18next';
import { createI18n } from './i18n/create-i18n.ts';
import { UiRoot } from './UiRoot.tsx';
import type { ExternalStore, UiSnapshot } from './ui-snapshot.ts';
import './ui.css';

/** What `mountUi` needs, injected by the composition root. */
export interface UiDeps {
  store: ExternalStore<UiSnapshot>;
  settings: SettingsStorePort;
}

/** Handle returned by {@link mountUi}. */
export interface MountedUi {
  unmount(): void;
}

/**
 * Mounts the player UI in its own React root with its own i18next instance (locale from the injected
 * settings). Call `unmount()` to tear both down.
 */
export function mountUi(container: HTMLElement, deps: UiDeps): MountedUi {
  const i18n = createI18n(deps.settings.load().locale);
  const root = createRoot(container);
  root.render(
    <StrictMode>
      <I18nextProvider i18n={i18n}>
        <UiRoot store={deps.store} />
      </I18nextProvider>
    </StrictMode>,
  );
  return { unmount: () => root.unmount() };
}
