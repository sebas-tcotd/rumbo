/** Fixed-step debt measured in milliseconds. Scheduling belongs to the worker. */
export class FixedClock {
  readonly stepMs: number;
  remainder = 0;
  private last: number | null = null;

  constructor(hz: number) {
    if (hz !== 120) throw new Error('UNSUPPORTED_TICK_RATE');
    this.stepMs = 1000 / hz;
  }

  start(now: number) {
    this.last = now;
    this.remainder = 0;
  }

  stop() {
    this.last = null;
    this.remainder = 0;
  }

  advance(now: number): number {
    if (this.last === null) return 0;
    const elapsed = this.remainder + Math.max(0, now - this.last);
    this.last = now;
    const due = Math.floor(elapsed / this.stepMs);
    this.remainder = elapsed - due * this.stepMs;
    return Math.min(8, due);
  }
}
