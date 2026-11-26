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

/** Where the gate writes: the terminal's queue, which writes one piece at a time, in order. */
export interface GateOutput {
  push(data: string | Uint8Array, done?: () => void): void;
}

/**
 * Joins a snapshot with the live event stream. Events that arrive before the
 * snapshot is applied are held back, and chunks the snapshot already contains
 * are skipped, so nothing is lost or painted twice. The snapshot's pieces and the
 * live output go to the same queue, in that order, so the terminal keeps them in order.
 */
export class TerminalStreamGate {
  private lastSeq = 0;
  private attached = false;
  private exited = false;
  private queued: StreamItem[] = [];

  constructor(
    private readonly output: GateOutput,
    private readonly onExit: (exitCode: number | null) => void,
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
    if (this.attached) return;
    const slices = sliceReplay(snapshot.data);
    if (slices.length === 0) onReplayed();
    slices.forEach((slice, index) => {
      this.output.push(
        slice,
        index === slices.length - 1 ? onReplayed : undefined,
      );
    });
    this.lastSeq = snapshot.lastSeq;
    this.attached = true;

    for (const item of this.queued) this.deliver(item);
    this.queued = [];

    // The process may have exited before we started listening.
    if (!snapshot.running) this.deliverExit(snapshot.exitCode);
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
    this.output.push(item.chunk.data);
  }

  private deliverExit(exitCode: number | null): void {
    if (this.exited) return;
    this.exited = true;
    this.onExit(exitCode);
  }
}
