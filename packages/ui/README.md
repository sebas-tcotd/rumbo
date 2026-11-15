# @rumbo/ui

La interfaz muestra el HUD, la carga, los avisos y la pantalla «Algo falló».
`apps/web/src/main.tsx` llama a `mountUi(container, { store, settings, fatal })`, que crea una raíz React
en StrictMode y una instancia de i18next, y devuelve `{ unmount }`.

La pantalla Fatal traduce `FatalView.code`, enumera las funciones requeridas ausentes y enfoca
«Reintentar». El host inyecta la recarga, la generación del diagnóstico y el acceso al portapapeles.
Si falla la copia, el diagnóstico queda seleccionado en un campo de solo lectura.

- Reads the host through structural types (`UiSnapshot`, `ExternalStore`), never importing the app. The HUD
  uses the ~15 Hz state capture, never per-frame state.
- i18n: `i18n/es.json`, keys `screen.element` in camelCase (`hud.bestLap`, `fatal.title`); notices arrive as
  codes and render as `errors.<CODE>`. `i18n/keys.test.mjs` checks the key format and that every key used in
  the source exists. No language detector and no HTTP backend.
- The one exception to camelCase (decided in the 0.6 review): `errors.<CODE>` keeps the code's UPPER_SNAKE
  name (`errors.SIM_NOT_READY`), because the code is the identifier of the error catalogue (story 0.8) and
  the UI translates it without a code→key map. New error codes follow the same rule.
- Settings only through `SettingsStorePort` (`locale`); never `localStorage`.
- Imports: `@rumbo/contracts`, `@rumbo/game` (future), react, react-dom, i18next, react-i18next
  (Biome `noRestrictedImports`, ADR-005).
