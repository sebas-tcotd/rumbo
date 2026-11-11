/** Languages with a complete UI resource bundle. */
export const SUPPORTED_LOCALES = ['es'] as const;
/** A supported UI language. */
export type Locale = (typeof SUPPORTED_LOCALES)[number];

/**
 * Player settings stored under localStorage `rumbo.settings.v1` (AR19).
 *
 * Every later story that persists a setting adds its field here, bumps SETTINGS_VERSION and adds a pure
 * `vN → vN+1` migration in `@rumbo/platform-web/storage`. Add a field only when a feature reads it.
 */
export interface Settings {
  /** UI language; consumed by `@rumbo/ui` to pick its i18n resources. */
  locale: Locale;
}

/** Current schema version of {@link Settings}. */
export const SETTINGS_VERSION = 1;

/** Values used when nothing valid is stored. */
export const DEFAULT_SETTINGS: Settings = { locale: 'es' };
