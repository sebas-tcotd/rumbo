# @rumbo/render-three

three.js adapter of `RendererPort` (WebGPU with WebGL 2 fallback). Built by `createThreeRenderer(container)`
in `apps/web/src/main.tsx`; nobody else instantiates it.

- No loop of its own: the host's single `requestAnimationFrame` calls `frame(prev, curr, alpha)` after
  `await renderer.init()`. `prev`/`curr` are host scratch; the renderer keeps no reference to them.
- Lab-only controls (`setCameraMode`, `setGhostVisible`, `setSuspensionRest`, `backend`) are used by
  `apps/web`; `@rumbo/ui` and `@rumbo/devtools` never import this package.
- Decoration uses `mulberry32` with a fixed seed: cosmetic only, never the simulation's PCG.
- Imports: `@rumbo/contracts` and `three` only (Biome `noRestrictedImports`, ADR-005).
