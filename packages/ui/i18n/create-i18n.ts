import type { Locale } from '@rumbo/contracts';
import i18next, { type i18n } from 'i18next';
import { initReactI18next } from 'react-i18next';
import es from './es.json';

/** Bundled resources: no HTTP backend (the CSP of 0.12 allows only same-origin connects). */
export const RESOURCES = { es: { translation: es } } as const;

/**
 * A fresh i18next instance per mount (never the global one), initialised synchronously from the
 * bundled resources so the first render already has every text.
 */
export function createI18n(locale: Locale): i18n {
  const instance = i18next.createInstance();
  instance.use(initReactI18next).init({
    lng: locale,
    fallbackLng: 'es',
    resources: RESOURCES,
    initAsync: false,
    // React already escapes interpolated values.
    interpolation: { escapeValue: false },
  });
  return instance;
}
