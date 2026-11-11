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

## Debug tools (AR28)

The «Depuración» tab and the Session tab add:

- **Overlays** of the centerline, checkpoints and collision boxes (chassis box + walls wireframe), through the
  injected `RenderDebugPort`; the renderer builds each one the first time it is switched on.
- **Teleport** to a lap distance `s` (worker command `devTeleport`). Not recorded in the replay: the session
  becomes *altered*, recording a golden is blocked and the replay section warns.
- **Record a golden**: downloads the current session's `race-lab-replay/1` as `<track>.<case>.json`, the format
  of `crates/sim-core/tests/golden/`. Adding it to `CASES` in `golden.rs` is a separate change under the golden
  rules; this tool adds nothing to the set.
- **Jump to `firstMismatchTick`**: after verifying a file that diverges, «Ir al tick N» re-simulates that replay in
  the lab up to the first mismatched check and pauses there (`devSeekReplay`, bounded by `DEV_SEEK_MAX_CHARS` and
  `DEV_SEEK_MAX_TICKS`; the worker rejects anything else as `INVALID_REPLAY`). The session becomes altered and the
  replay's own profile becomes the applied one (tuning panel, car rest height, «Reiniciar sesión»).
- **Teleport on a sprint** past the start line leaves no timed attempt until T: the sprint only starts timing when
  the car crosses the start going forward. The Debug tab says so; the timing rules are not changed for dev tools.

Out of scope here: the provenance overlay (2.3) and compiler diagnostics on the map (2.6).
