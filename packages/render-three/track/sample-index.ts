/** Centerline sample nearest to lap distance `s` (wrapping on circuits, clamped on sprints). */
export function sampleIndexAt(s: number, spacing: number, count: number, closed: boolean): number {
  const raw = Math.round(s / spacing);
  return closed ? ((raw % count) + count) % count : Math.min(count - 1, Math.max(0, raw));
}
