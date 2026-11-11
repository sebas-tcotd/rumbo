import type { ProfileFile, ReadyMsg, StateIndex, StateMsg } from '@rumbo/contracts';

/** Codes of non-fatal notices; the UI translates them as `errors.<code>`. */
export type UiNoticeCode = 'SIM_NOT_READY' | 'INVALID_PROFILE' | 'EXPORT_FAILED' | 'UNSUPPORTED_OPERATION';

/** A rejected command the player should know about (the session keeps running). */
export interface UiNotice {
  code: UiNoticeCode;
  /** Operation name, interpolated by UNSUPPORTED_OPERATION. */
  operation?: string;
}

/**
 * What the UI reads from the host, structurally: apps/web's lab store satisfies it without ui
 * importing the app. `state` is a low-frequency (~15 Hz) copy, never the per-frame scratch.
 */
export interface UiSnapshot {
  ready: ReadyMsg | null;
  index: StateIndex | null;
  state: StateMsg | null;
  appliedProfile: ProfileFile | null;
  /** Best-lap ghost frames, for the HUD delta. */
  ghost: Float32Array | null;
  autopilot: boolean;
  paused: boolean;
  /** False until the renderer finished `init()`. */
  rendererReady: boolean;
  notice: UiNotice | null;
  /** Technical message of a fatal error, shown as is until the Fatal screen of 0.8. */
  error: string | null;
}

/** Minimal external store (the shape `useSyncExternalStore` consumes). */
export interface ExternalStore<T> {
  subscribe(listener: () => void): () => void;
  getSnapshot(): T;
}
