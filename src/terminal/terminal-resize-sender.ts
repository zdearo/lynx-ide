export interface TerminalSize {
  cols: number;
  rows: number;
}

/** A terminal's size sender: call it with each size, and hold it while the process must keep its size. */
export interface ResizeSender {
  (size: TerminalSize): void;
  /** While held, sizes are kept and nothing is sent; releasing sends the latest one. */
  hold(held: boolean): void;
}

/**
 * Sends a terminal's size to its process one request at a time, and only the latest size while
 * one is in flight. Backend commands run in parallel, so two quick resizes could land out of
 * order and leave the process with a size the screen no longer has.
 */
export function createResizeSender(
  send: (size: TerminalSize) => Promise<unknown>,
): ResizeSender {
  let sending = false;
  let held = false;
  let latest: TerminalSize | null = null;

  async function drain() {
    sending = true;
    while (latest && !held) {
      const size = latest;
      latest = null;
      try {
        await send(size);
      } catch {
        // The session may be gone already; a newer size, if any, still goes out.
      }
    }
    sending = false;
  }

  const resize = ((size: TerminalSize) => {
    latest = size;
    if (!sending && !held) void drain();
  }) as ResizeSender;
  resize.hold = (next: boolean) => {
    held = next;
    if (!held && !sending) void drain();
  };
  return resize;
}
