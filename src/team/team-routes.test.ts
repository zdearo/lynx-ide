import { describe, expect, it } from "vitest";

import { TEAM_CARD_WIDTH as W } from "@/team/team-layout";
import {
  orchestrationRoute,
  pairRoute,
  roundedPath,
  routeIsClear,
} from "@/team/team-routes";

const ROW = 176;

describe("orchestrationRoute", () => {
  it("runs along the gap between rows for a member on the next row", () => {
    const route = orchestrationRoute({ x: 0, y: 0 }, { x: 300, y: ROW }, []);
    expect(route[0]).toEqual({ x: W / 2, y: 112 });
    expect(route.at(-1)).toEqual({ x: 300 + W / 2, y: ROW });
    expect(route.every((p) => p.y >= 112 && p.y <= ROW)).toBe(true);
  });

  it("never goes behind a card on a row in between", () => {
    const blocker = { x: 0, y: ROW };
    const route = orchestrationRoute({ x: 0, y: 0 }, { x: 0, y: 2 * ROW }, [
      blocker,
    ]);
    expect(routeIsClear(route, [blocker])).toBe(true);
    expect(route.length).toBeGreaterThan(2);
  });
});

describe("pairRoute", () => {
  const left = { x: 0, y: 0 };
  const right = { x: 300, y: 0 };

  it("is straight between neighbours", () => {
    expect(pairRoute(left, right, [])).toHaveLength(2);
  });

  it("runs over a card that stands between the two", () => {
    const between = { x: 150, y: 0 };
    const route = pairRoute(left, { x: 600, y: 0 }, [between]);
    expect(routeIsClear(route, [between])).toBe(true);
    expect(Math.min(...route.map((p) => p.y))).toBeLessThan(0);
  });
});

describe("roundedPath", () => {
  it("starts and ends on the first and last point", () => {
    const d = roundedPath([
      { x: 0, y: 0 },
      { x: 0, y: 50 },
      { x: 80, y: 50 },
    ]);
    expect(d.startsWith("M 0 0")).toBe(true);
    expect(d.endsWith("L 80 50")).toBe(true);
  });
});
