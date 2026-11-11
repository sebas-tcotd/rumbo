# @rumbo/platform-web

Browser adapters of the ports in `@rumbo/contracts`. `createWebPlatform()` is called once by
`apps/web/src/main.tsx`; no other package instantiates these adapters.

| Folder | Port | State |
|---|---|---|
| `input/` | `InputSourcePort` | Keyboard + standard gamepad (`createKeyboardGamepadInput`) |
| `storage/` | `SettingsStorePort` | `localStorage` `rumbo.settings.v1` (`createLocalSettingsStore`) |
| `audio/` | `AudioPort` | Empty until story 5.1 |
| `telemetry/` | `TelemetryPort` | Empty |
| `zone-source/` | `ZonePackSourcePort` | Empty until story 0.12 |

## Settings schema (`rumbo.settings.v1`, AR19)

Value: `{ "version": N, ...fields }`; v1 holds only `locale` (read by `@rumbo/ui`).

**Every later story that stores a setting adds it to this schema**: extend `Settings` in
`@rumbo/contracts/settings.ts`, bump `SETTINGS_VERSION`, append a pure `vN → vN+1` step to `MIGRATIONS` in
`storage/settings-schema.ts`, validate the field in `sanitize`, and add a fixture test. Never a new
`localStorage` key. Corrupt data or an unreadable storage yields defaults; data from a newer build is
read as defaults and is not overwritten.

Imports: `@rumbo/contracts` only (Biome `noRestrictedImports`, ADR-005). The subfolders (`audio/`, `input/`,
`storage/`, `telemetry/`, `zone-source/`) are isolated: none imports another or the package root; only the
root files (`index.ts`, `web-platform.ts`) wire them. A new subfolder is added to `isolatedSubfolders` in
`ci/layers/policy.mjs`.
