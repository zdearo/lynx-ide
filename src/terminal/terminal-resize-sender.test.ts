import { describe, expect, it } from "vitest";

import { createResizeSender } from "@/terminal/terminal-resize-sender";

/** A send that finishes only when the test says so. */
function controlledSend() {
  const sent: string[] = [];
  const releases: (() => void)[] = [];
  const send = ({ cols, rows }: { cols: number; rows: number }) => {
    sent.push(`${cols}x${rows}`);
    return new Promise<void>((resolve) => releases.push(resolve));
  };
  return { sent, send, release: () => releases.shift()?.() };
}

const flush = () => new Promise((resolve) => setTimeout(resolve, 0));

describe("createResizeSender", () => {
  it("sends the first size at once", () => {
    const { sent, send } = controlledSend();
    createResizeSender(send)({ cols: 100, rows: 30 });
    expect(sent).toEqual(["100x30"]);
  });

  it("waits for the request in flight, and then sends only the latest size", async () => {
    const { sent, send, release } = controlledSend();
    const resize = createResizeSender(send);

    resize({ cols: 100, rows: 30 });
    resize({ cols: 120, rows: 30 });
    resize({ cols: 180, rows: 30 });
    expect(sent).toEqual(["100x30"]);

    release();
    await flush();
    expect(sent).toEqual(["100x30", "180x30"]);
  });

  it("keeps going after a request fails", async () => {
    const sent: string[] = [];
    const resize = createResizeSender(({ cols }) => {
      sent.push(String(cols));
      return cols === 100
        ? Promise.reject(new Error("gone"))
        : Promise.resolve();
    });

    resize({ cols: 100, rows: 30 });
    resize({ cols: 150, rows: 30 });
    await flush();
    expect(sent).toEqual(["100", "150"]);

    resize({ cols: 160, rows: 30 });
    expect(sent).toEqual(["100", "150", "160"]);
  });
});

describe("createResizeSender hold", () => {
  it("keeps the sizes while held and sends the latest one on release", async () => {
    const sent: string[] = [];
    const resize = createResizeSender(async ({ cols, rows }) => {
      sent.push(`${cols}x${rows}`);
    });

    resize.hold(true);
    resize({ cols: 100, rows: 30 });
    resize({ cols: 180, rows: 30 });
    await flush();
    expect(sent).toEqual([]);

    resize.hold(false);
    await flush();
    expect(sent).toEqual(["180x30"]);
  });

  it("sends nothing on release when no size came during the hold", async () => {
    const sent: string[] = [];
    const resize = createResizeSender(async ({ cols, rows }) => {
      sent.push(`${cols}x${rows}`);
    });

    resize.hold(true);
    resize.hold(false);
    await flush();
    expect(sent).toEqual([]);
  });
});

describe("createResizeSender sizes", () => {
  it("does not send again the size it was last asked for", async () => {
    const sent: Array<{ cols: number; rows: number }> = [];
    const resize = createResizeSender(async (size) => {
      sent.push(size);
    });

    resize({ cols: 80, rows: 24 });
    resize({ cols: 80, rows: 24 });
    await new Promise((done) => setTimeout(done, 0));
    resize({ cols: 80, rows: 24 });
    await new Promise((done) => setTimeout(done, 0));

    expect(sent).toEqual([{ cols: 80, rows: 24 }]);
  });

  it("sends a size that changed, and sends the same size again after forget", async () => {
    const sent: Array<{ cols: number; rows: number }> = [];
    const resize = createResizeSender(async (size) => {
      sent.push(size);
    });

    resize({ cols: 80, rows: 24 });
    await new Promise((done) => setTimeout(done, 0));
    resize({ cols: 100, rows: 24 });
    await new Promise((done) => setTimeout(done, 0));
    resize.forget();
    resize({ cols: 100, rows: 24 });
    await new Promise((done) => setTimeout(done, 0));

    expect(sent).toEqual([
      { cols: 80, rows: 24 },
      { cols: 100, rows: 24 },
      { cols: 100, rows: 24 },
    ]);
  });
});
