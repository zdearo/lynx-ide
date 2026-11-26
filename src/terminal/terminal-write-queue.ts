/**
 * Everything written to one terminal, one write at a time, in order: each write starts in a frame of its own, once the
 * terminal has parsed the one before it. So a long replay never fills the terminal's parser at once.
 */

export type WriteData = string | Uint8Array;

interface Pending {
  data: WriteData;
  done?: () => void;
}

export class TerminalWriteQueue {
  private readonly pending: Pending[] = [];
  private busy = false;
  private scheduled = false;
  private cancelRun: (() => void) | null = null;

  constructor(
    /** Writes to the terminal; `done` is called once the terminal has parsed the data. */
    private readonly write: (data: WriteData, done: () => void) => void,
    private readonly schedule: (run: () => void) => () => void = nextFrame,
  ) {}

  push(data: WriteData, done?: () => void): void {
    this.pending.push({ data, done });
    this.kick();
  }

  /** Drops the writes that have not started. The one in progress finishes. */
  clear(): void {
    this.pending.length = 0;
    if (this.scheduled) {
      this.cancelRun?.();
      this.scheduled = false;
    }
  }

  private kick(): void {
    if (this.scheduled || this.busy || this.pending.length === 0) return;
    this.scheduled = true;
    this.cancelRun = this.schedule(() => {
      this.scheduled = false;
      this.pump();
    });
  }

  // One write per frame: the next one waits for the parse callback, and then for the frame kick() schedules.
  private pump(): void {
    if (this.busy) return;
    const next = this.pending.shift();
    if (!next) return;
    this.busy = true;
    this.write(next.data, () => {
      this.busy = false;
      next.done?.();
      this.kick();
    });
  }
}

function nextFrame(run: () => void): () => void {
  const frame = requestAnimationFrame(run);
  return () => cancelAnimationFrame(frame);
}
