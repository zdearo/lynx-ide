import { describe, expect, it, vi } from "vitest";

import { TerminalWriteQueue } from "@/terminal/terminal-write-queue";

// A terminal that parses when the test says so: `parsed` holds the callbacks of the writes still waiting for it.
function parsingTerminal() {
  const written: string[] = [];
  const parsed: Array<() => void> = [];
  const write = (data: string | Uint8Array, done: () => void) => {
    written.push(data as string);
    parsed.push(done);
  };
  return { written, parsed, write };
}

// Frames run when the test runs them: `frames` holds the work waiting for the next one.
function framesByHand() {
  const frames: Array<() => void> = [];
  const schedule = (run: () => void) => {
    frames.push(run);
    return () => {};
  };
  return { frames, schedule };
}

describe("TerminalWriteQueue", () => {
  it("starts each write only once the one before it has been parsed", () => {
    const terminal = parsingTerminal();
    const queue = new TerminalWriteQueue(terminal.write, (run) => {
      run();
      return () => {};
    });

    queue.push("a");
    queue.push("b");
    queue.push("c");
    expect(terminal.written).toEqual(["a"]);

    terminal.parsed.shift()?.();
    expect(terminal.written).toEqual(["a", "b"]);
    terminal.parsed.shift()?.();
    expect(terminal.written).toEqual(["a", "b", "c"]);
  });

  it("calls each write's done once the terminal has parsed it", () => {
    const terminal = parsingTerminal();
    const queue = new TerminalWriteQueue(terminal.write, (run) => {
      run();
      return () => {};
    });
    const done = vi.fn();

    queue.push("a", done);
    expect(done).not.toHaveBeenCalled();
    terminal.parsed.shift()?.();
    expect(done).toHaveBeenCalledOnce();
  });

  it("drops the writes that have not started when cleared, and lets the one in progress finish", () => {
    const terminal = parsingTerminal();
    const queue = new TerminalWriteQueue(terminal.write, (run) => {
      run();
      return () => {};
    });

    queue.push("a");
    queue.push("b");
    queue.clear();
    terminal.parsed.shift()?.();

    expect(terminal.written).toEqual(["a"]);
  });

  it("starts the next write in a later frame, not in the one where the last was parsed", () => {
    const terminal = parsingTerminal();
    const frames = framesByHand();
    const queue = new TerminalWriteQueue(terminal.write, frames.schedule);

    queue.push("a");
    queue.push("b");
    frames.frames.shift()?.();
    expect(terminal.written).toEqual(["a"]);

    terminal.parsed.shift()?.();
    expect(terminal.written).toEqual(["a"]);
    frames.frames.shift()?.();
    expect(terminal.written).toEqual(["a", "b"]);
  });

  it("writes in order, one per frame, each once the terminal has parsed the one before", () => {
    const terminal = parsingTerminal();
    const frames = framesByHand();
    const queue = new TerminalWriteQueue(terminal.write, frames.schedule);

    queue.push("a");
    queue.push("b");
    queue.push("c");
    frames.frames.shift()?.();
    expect(terminal.written).toEqual(["a"]);

    terminal.parsed.shift()?.();
    expect(terminal.written).toEqual(["a"]);
    frames.frames.shift()?.();
    expect(terminal.written).toEqual(["a", "b"]);

    terminal.parsed.shift()?.();
    frames.frames.shift()?.();
    expect(terminal.written).toEqual(["a", "b", "c"]);
  });
});
