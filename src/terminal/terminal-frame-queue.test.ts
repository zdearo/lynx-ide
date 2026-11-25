import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";

import { queueTerminalFrame } from "@/terminal/terminal-frame-queue";

describe("queueTerminalFrame", () => {
  let frames: FrameRequestCallback[] = [];

  beforeEach(() => {
    frames = [];
    vi.stubGlobal("requestAnimationFrame", (callback: FrameRequestCallback) => {
      frames.push(callback);
      return frames.length;
    });
  });

  afterEach(() => {
    vi.unstubAllGlobals();
  });

  // Runs the frame that is pending, if any; returns how many frames were pending.
  function nextFrame(): number {
    const pending = frames;
    frames = [];
    pending.forEach((callback) => callback(0));
    return pending.length;
  }

  it("runs one piece of work per frame, in the order they were queued", () => {
    const ran: string[] = [];
    queueTerminalFrame(() => ran.push("a"));
    queueTerminalFrame(() => ran.push("b"));
    queueTerminalFrame(() => ran.push("c"));

    nextFrame();
    expect(ran).toEqual(["a"]);
    nextFrame();
    expect(ran).toEqual(["a", "b"]);
    nextFrame();
    expect(ran).toEqual(["a", "b", "c"]);
    expect(nextFrame()).toBe(0);
  });

  it("drops work that was cancelled before its frame", () => {
    const ran: string[] = [];
    queueTerminalFrame(() => ran.push("a"));
    const cancel = queueTerminalFrame(() => ran.push("b"));
    queueTerminalFrame(() => ran.push("c"));
    cancel();

    while (nextFrame() > 0) {
      // run the frames until the queue is empty
    }
    expect(ran).toEqual(["a", "c"]);
  });

  it("keeps a single frame pending when work queues more work while it runs", () => {
    const ran: string[] = [];
    queueTerminalFrame(() => {
      ran.push("a");
      queueTerminalFrame(() => ran.push("nested"));
    });
    queueTerminalFrame(() => ran.push("b"));

    expect(nextFrame()).toBe(1);
    expect(ran).toEqual(["a"]);
    expect(nextFrame()).toBe(1);
    expect(ran).toEqual(["a", "b"]);
    expect(nextFrame()).toBe(1);
    expect(ran).toEqual(["a", "b", "nested"]);
    expect(nextFrame()).toBe(0);
  });
});
