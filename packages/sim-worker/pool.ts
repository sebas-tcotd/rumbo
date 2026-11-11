import { SNAPSHOT_SLOTS } from '@rumbo/contracts';

/**
 * Bounded pool of transport slots (see SNAPSHOT_SLOTS in @rumbo/contracts for the ownership rules).
 * A transferred slot becomes available again only when the host returns it; `take` never allocates.
 */
export class SnapshotPool {
  private readonly buffers: (ArrayBuffer | null)[];
  readonly byteLength: number;
  constructor(byteLength: number) {
    this.byteLength = byteLength;
    this.buffers = Array.from({ length: SNAPSHOT_SLOTS }, () => new ArrayBuffer(byteLength));
  }
  take(): { slot: number; buffer: ArrayBuffer } | null {
    const slot = this.buffers.findIndex((buffer) => buffer !== null);
    if (slot < 0) return null;
    const buffer = this.buffers[slot]!;
    this.buffers[slot] = null;
    return { slot, buffer };
  }
  release(slot: number, buffer: ArrayBuffer) {
    if (this.buffers[slot] !== null || buffer.byteLength !== this.byteLength)
      throw new Error('INVALID_BUFFER_RETURN');
    this.buffers[slot] = buffer;
  }
}
