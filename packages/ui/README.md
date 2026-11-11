# @rumbo/ui

Player-facing UI over the 3D view: HUD, loading, notices and the provisional fatal state (the real Fatal
screen is story 0.8). `mountUi(container, { store, settings })` is called once by `apps/web/src/main.tsx`;
it creates its own React root (StrictMode) and its own i18next instance and returns `{ unmount }`.

- Reads the host through structural types (`UiSnapshot`, `ExternalStore`), never importing the app. The HUD
  uses the ~15 Hz state capture, never per-frame state.
- i18n: `i18n/es.json`, keys `screen.element` in camelCase (`hud.bestLap`, `fatal.title`); notices arrive as
  codes and render as `errors.<CODE>`. `i18n/keys.test.mjs` checks the key format and that every key used in
  the source exists. No language detector and no HTTP backend.
- Settings only through `SettingsStorePort` (`locale`); never `localStorage`.
- Imports: `@rumbo/contracts`, `@rumbo/game` (future), react, react-dom, i18next, react-i18next
  (Biome `noRestrictedImports`, ADR-005).
