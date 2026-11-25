/**
 * Heavy terminal work (creating a terminal, asking for its output, drawing with the GPU) runs one item per frame, so
 * opening several terminals spreads the cost over frames instead of putting it all in one.
 */
interface QueuedWork {
  run: () => void;
  cancelled: boolean;
}

const queue: QueuedWork[] = [];
let frame: number | null = null;

function runNext() {
  frame = null;
  const next = queue.shift();
  if (next && !next.cancelled) next.run();
  // Work queued while this item ran has already asked for a frame; only one frame may be pending.
  if (queue.length > 0 && frame === null) {
    frame = requestAnimationFrame(runNext);
  }
}

/** Queues one piece of terminal work for a later frame. The function returned drops it if it has not run yet. */
export function queueTerminalFrame(run: () => void): () => void {
  const item: QueuedWork = { run, cancelled: false };
  queue.push(item);
  if (frame === null) frame = requestAnimationFrame(runNext);
  return () => {
    item.cancelled = true;
  };
}
