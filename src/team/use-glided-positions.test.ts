import { describe, expect, it } from "vitest";

import { glidePositions } from "@/team/use-glided-positions";

const from = new Map([["a", { x: 0, y: 0 }]]);
const to = new Map([
  ["a", { x: 100, y: 200 }],
  ["b", { x: 50, y: 50 }],
]);

describe("glidePositions", () => {
  it("starts where the card was and ends on its target", () => {
    expect(glidePositions(from, to, 0).get("a")).toEqual({ x: 0, y: 0 });
    expect(glidePositions(from, to, 1).get("a")).toEqual({ x: 100, y: 200 });
  });

  it("is between the two, and already past halfway at half time (easing out)", () => {
    const middle = glidePositions(from, to, 0.5).get("a");
    expect(middle?.y).toBeGreaterThan(100);
    expect(middle?.y).toBeLessThan(200);
  });

  it("puts a card that had no start straight on its target", () => {
    expect(glidePositions(from, to, 0).get("b")).toEqual({ x: 50, y: 50 });
  });
});
