import { GHOST_STRIDE } from './protocol.js';

/**
 * Interpolation factor between the two latest ticks. `nowAbsMs` and `atMs` are absolute times
 * (`performance.timeOrigin + performance.now()`, like `StateMsg.at`); mixing in a relative
 * `performance.now()` silently breaks interpolation. Clamped to [0, 1]; a non-finite input yields 1.
 */
export function interpolationAlpha(nowAbsMs: number, atMs: number, tickHz: number): number {
  const alpha = (nowAbsMs - atMs) / (1000 / tickHz);
  if (!Number.isFinite(alpha)) return 1;
  return Math.min(1, Math.max(0, alpha));
}

/**
 * Ghost time (ticks into its lap) when it reached lap distance `s`, by binary search over the
 * GHOST_STRIDE pose frames (`s` is field 7). Null without a ghost of at least two frames.
 */
export function ghostTicksAt(frames: Float32Array | null, s: number): number | null {
  if (!frames) return null;
  const n = Math.floor(frames.length / GHOST_STRIDE);
  if (n < 2) return null;
  let lo = 0;
  let hi = n - 1;
  // `s` grows along the lap except for the finish-line wrap on the first frame.
  while (lo < hi) {
    const mid = (lo + hi) >> 1;
    if (frames[mid * GHOST_STRIDE + 7] < s) lo = mid + 1;
    else hi = mid;
  }
  return lo;
}
