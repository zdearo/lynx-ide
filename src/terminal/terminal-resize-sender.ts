export interface TerminalSize {
  cols: number;
  rows: number;
}

/**
 * Sends a terminal's size to its process one request at a time, and only the latest size while
 * one is in flight. Backend commands run in parallel, so two quick resizes could land out of
 * order and leave the process with a size the screen no longer has.
 */
export function createResizeSender(
  send: (size: TerminalSize) => Promise<unknown>,
): (size: TerminalSize) => void {
  let sending = false;
  let latest: TerminalSize | null = null;

  async function drain() {
    sending = true;
    while (latest) {
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

  return (size) => {
    latest = size;
    if (!sending) void drain();
  };
}
