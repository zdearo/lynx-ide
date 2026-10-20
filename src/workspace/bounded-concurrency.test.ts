import { describe, expect, it } from "vitest";

import { forEachBounded } from "@/workspace/bounded-concurrency";

const tick = () => new Promise<void>((resolve) => setTimeout(resolve, 0));

describe("forEachBounded", () => {
  it("never runs more than the limit at once and still visits every item", async () => {
    let running = 0;
    let peak = 0;
    const seen: number[] = [];

    await forEachBounded([1, 2, 3, 4, 5, 6, 7], 3, async (item) => {
      running++;
      peak = Math.max(peak, running);
      await tick();
      seen.push(item);
      running--;
    });

    expect(peak).toBe(3);
    expect(seen.sort()).toEqual([1, 2, 3, 4, 5, 6, 7]);
  });

  it("starts items in order", async () => {
    const started: number[] = [];

    await forEachBounded([1, 2, 3, 4], 1, async (item) => {
      started.push(item);
    });

    expect(started).toEqual([1, 2, 3, 4]);
  });

  it("finishes the other items and then throws the first failure", async () => {
    const done: number[] = [];

    await expect(
      forEachBounded([1, 2, 3], 2, async (item) => {
        if (item === 2) throw new Error("boom");
        done.push(item);
      }),
    ).rejects.toThrow("boom");

    expect(done.sort()).toEqual([1, 3]);
  });

  it("does nothing for an empty list", async () => {
    await expect(
      forEachBounded([], 4, () => Promise.reject(new Error("never"))),
    ).resolves.toBeUndefined();
  });
});
