import { describe, expect, it } from "vitest";

import { decodeOutputFrame } from "@/terminal/terminal-output-stream";

function frame(seq: number, text: string): ArrayBuffer {
  const body = new TextEncoder().encode(text);
  const bytes = new Uint8Array(4 + body.length);
  new DataView(bytes.buffer).setUint32(0, seq, true);
  bytes.set(body, 4);
  return bytes.buffer;
}

describe("decodeOutputFrame", () => {
  it("reads the sequence number and leaves the text as bytes", () => {
    const chunk = decodeOutputFrame(frame(0x0102_0304, "ção\u001b[0m"));

    expect(chunk?.seq).toBe(0x0102_0304);
    expect(new TextDecoder().decode(chunk?.data as Uint8Array)).toBe(
      "ção\u001b[0m",
    );
  });

  it("reads an empty chunk", () => {
    const chunk = decodeOutputFrame(frame(7, ""));

    expect(chunk?.seq).toBe(7);
    expect((chunk?.data as Uint8Array).length).toBe(0);
  });

  it("ignores a frame too short to hold a sequence number", () => {
    expect(decodeOutputFrame(new ArrayBuffer(3))).toBeNull();
  });
});
