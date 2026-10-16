export interface OutputChunk {
  seq: number;
  data: string;
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

/**
 * Joins a snapshot with the live event stream. Events that arrive before the
 * snapshot is applied are held back, and chunks the snapshot already contains
 * are skipped, so nothing is lost or painted twice.
 */
export class TerminalStreamGate {
  private lastSeq = 0;
  private attached = false;
  private exited = false;
  private queued: StreamItem[] = [];

  constructor(
    private readonly write: (data: string) => void,
    private readonly onExit: (exitCode: number | null) => void,
  ) {}

  pushOutput(chunk: OutputChunk): void {
    this.accept({ kind: "output", chunk });
  }

  pushExit(exitCode: number | null): void {
    this.accept({ kind: "exit", exitCode });
  }

  attach(snapshot: TerminalSnapshotState): void {
    if (this.attached) return;
    if (snapshot.data) this.write(snapshot.data);
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
    this.write(item.chunk.data);
  }

  private deliverExit(exitCode: number | null): void {
    if (this.exited) return;
    this.exited = true;
    this.onExit(exitCode);
  }
}
