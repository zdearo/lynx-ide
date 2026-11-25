export interface OutputChunk {
  seq: number;
  /** Text, or the bytes of text the terminal decodes itself. */
  data: string | Uint8Array;
}

export interface TerminalSnapshotState {
  data: string;
  lastSeq: number;
  running: boolean;
  exitCode: number | null;
}

type StreamItem =
  | { kind: "output"; chunk: OutputChunk }
  | { kind: "exit"; exitCode: number | null };

/** The longest piece of a replay the terminal is given at once: it parses a piece in one go, so pieces keep it short. */
export const REPLAY_SLICE = 64 * 1024;

/** Cuts a replay into pieces of at most REPLAY_SLICE, never between the two halves of a surrogate pair. */
function sliceReplay(data: string): string[] {
  const slices: string[] = [];
  let start = 0;
  while (start < data.length) {
    let end = Math.min(start + REPLAY_SLICE, data.length);
    const last = data.charCodeAt(end - 1);
    if (end < data.length && last >= 0xd800 && last <= 0xdbff) end -= 1;
    slices.push(data.slice(start, end));
    start = end;
  }
  return slices;
}

/**
 * Joins a snapshot with the live event stream. Events that arrive before the
 * snapshot is applied are held back, and chunks the snapshot already contains
 * are skipped, so nothing is lost or painted twice. The snapshot is written in
 * pieces, one per `later` step, so a long replay does not take one frame; the
 * live output waits behind the last piece.
 */
export class TerminalStreamGate {
  private lastSeq = 0;
  private attached = false;
  private replaying = false;
  private exited = false;
  private queued: StreamItem[] = [];
  private cancelReplay: (() => void) | null = null;

  constructor(
    private readonly write: (
      data: string | Uint8Array,
      done?: () => void,
    ) => void,
    private readonly onExit: (exitCode: number | null) => void,
    private readonly later: (step: () => void) => () => void = (step) => {
      step();
      return () => {};
    },
  ) {}

  pushOutput(chunk: OutputChunk): void {
    this.accept({ kind: "output", chunk });
  }

  pushExit(exitCode: number | null): void {
    this.accept({ kind: "exit", exitCode });
  }

  /** Applies the snapshot. `onReplayed` runs once the terminal has parsed all of it. */
  attach(
    snapshot: TerminalSnapshotState,
    onReplayed: () => void = () => {},
  ): void {
    if (this.attached || this.replaying) return;
    const slices = sliceReplay(snapshot.data);
    const finish = () => {
      this.replaying = false;
      this.attached = true;
      this.lastSeq = snapshot.lastSeq;
      for (const item of this.queued) this.deliver(item);
      this.queued = [];

      // The process may have exited before we started listening.
      if (!snapshot.running) this.deliverExit(snapshot.exitCode);
    };
    if (slices.length === 0) {
      finish();
      onReplayed();
      return;
    }

    this.replaying = true;
    let next = 0;
    const step = () => {
      const slice = slices[next];
      next += 1;
      if (next < slices.length) {
        this.write(slice);
        this.cancelReplay = this.later(step);
        return;
      }
      this.cancelReplay = null;
      this.write(slice, onReplayed);
      finish();
    };
    this.cancelReplay = this.later(step);
  }

  /** Stops a replay that has not finished: the terminal is going away. */
  dispose(): void {
    this.cancelReplay?.();
    this.cancelReplay = null;
  }

  private accept(item: StreamItem): void {
    if (this.attached) this.deliver(item);
    else this.queued.push(item);
  }

  private deliver(item: StreamItem): void {
    if (item.kind === "exit") {
      this.deliverExit(item.exitCode);
      return;
    }
    if (item.chunk.seq <= this.lastSeq) return;
    this.lastSeq = item.chunk.seq;
    this.write(item.chunk.data);
  }

  private deliverExit(exitCode: number | null): void {
    if (this.exited) return;
    this.exited = true;
    this.onExit(exitCode);
  }
}
