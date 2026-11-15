/// <reference types="vite/client" />

interface ImportMetaEnv {
  /** Build identifier included in player-copied diagnostics. */
  readonly RUMBO_BUILD: string;
  /** Build-time flag (vite.config.ts): true in `vite dev` and in the lab build, false in publication. */
  readonly RUMBO_DEVTOOLS: boolean;
}
