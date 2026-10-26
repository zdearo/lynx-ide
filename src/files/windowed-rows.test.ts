import { describe, expect, it } from "vitest";

import { indexesToDraw, windowRange } from "@/files/windowed-rows";

describe("windowRange", () => {
  it("covers the visible rows plus the overscan", () => {
    expect(windowRange({ first: 100, count: 30 }, 5000)).toEqual({
      start: 90,
      end: 140,
    });
  });

  it("stays inside the list", () => {
    expect(windowRange({ first: 0, count: 30 }, 12)).toEqual({
      start: 0,
      end: 12,
    });
    expect(windowRange({ first: 4990, count: 30 }, 5000)).toEqual({
      start: 4980,
      end: 5000,
    });
  });
});

describe("indexesToDraw", () => {
  const range = { start: 10, end: 14 };

  it("draws the window in order", () => {
    expect(indexesToDraw(range, 11)).toEqual([10, 11, 12, 13]);
  });

  it("keeps the pinned row when it is above or below the window", () => {
    expect(indexesToDraw(range, 2)).toEqual([2, 10, 11, 12, 13]);
    expect(indexesToDraw(range, 40)).toEqual([10, 11, 12, 13, 40]);
  });

  it("has nothing to pin when there is no pinned row", () => {
    expect(indexesToDraw(range, -1)).toEqual([10, 11, 12, 13]);
  });
});
