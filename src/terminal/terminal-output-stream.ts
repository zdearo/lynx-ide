import { Channel } from "@tauri-apps/api/core";

import { commands, type OutputBytes } from "@/bindings";
import type { OutputChunk } from "@/terminal/terminal-stream-gate";

const SEQ_BYTES = 4;

/** One chunk as the backend sends it: its `seq` (4 bytes, little-endian), then the output as UTF-8. */
export function decodeOutputFrame(frame: ArrayBuffer): OutputChunk | null {
  if (frame.byteLength < SEQ_BYTES) return null;
  return {
    seq: new DataView(frame).getUint32(0, true),
    // The terminal takes bytes as they are, and keeps a character split across two writes whole.
    data: new Uint8Array(frame, SEQ_BYTES),
  };
}

/**
 * Starts the output stream of a session and resolves with what it printed so far. Chunks reach
 * `onChunk` in order, as raw bytes: no JSON in between.
 */
export function attachTerminalOutput(
  id: string,
  onChunk: (chunk: OutputChunk) => void,
) {
  const channel = new Channel<ArrayBuffer>((frame) => {
    const chunk = decodeOutputFrame(frame);
    if (chunk) onChunk(chunk);
  });
  // The generated contract calls the bytes `number[]`; a channel delivers them as an ArrayBuffer.
  return commands.terminalAttach(
    id,
    channel as unknown as Channel<OutputBytes>,
  );
}
