/** Events available to race observers as their simulation capabilities come online. */
export type RaceEvent =
  | { type: 'lap.completed'; ticks: number }
  | { type: 'checkpoint.passed'; index: number }
  | { type: 'race.finished'; ticks: number };
