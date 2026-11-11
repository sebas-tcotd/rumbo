/// <reference types="vite/client" />

interface ImportMetaEnv {
  /** Build-time flag (vite.config.ts): true in `vite dev` and in the lab build, false in publication. */
  readonly RUMBO_DEVTOOLS: boolean;
}
