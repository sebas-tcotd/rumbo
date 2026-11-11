# @rumbo/devtools

Driving lab panels (Tuning, Telemetry, Session). Loaded by `apps/web/src/main.tsx` with a dynamic
`import()` behind the build flag `import.meta.env.RUMBO_DEVTOOLS`: on in `vite dev` and in the lab build
(`apps/web/dist-lab/`), off in the publication build (`apps/web/dist/`), which must not contain it
(`ci/bundle/check-devtools.mjs` looks for `DEVTOOLS_MARKER`).

- `mountDevtools({ panel, toolbar }, deps)` mounts its own React root (separate from `@rumbo/ui` on purpose).
- Reads the host through structural types (`DevSnapshot`, `DevActions`, `DevEvents`), never importing the app.
- Texts are literal Spanish: this is a developer tool, not player UI, so it is not translated.
- The tuning draft lives under `localStorage` `rumbo.devtools.draftProfile` (it is not a player setting, so
  it is not part of `rumbo.settings.v1`); the PoC key `race-lab:draft-profile` is still read once.
- Imports: `@rumbo/contracts`, `@rumbo/game` (future), react, react-dom (Biome `noRestrictedImports`, ADR-005).
  three is never imported; the renderer is reached through ports injected by the composition root.
