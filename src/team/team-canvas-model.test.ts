import { describe, expect, it } from "vitest";

import type { TeamMember } from "@/team/team-fixtures";
import { TEAM_CARD_HEIGHT, TEAM_CARD_WIDTH } from "@/team/team-layout";
import {
  boxOf,
  fitView,
  isInView,
  CARD_DEFAULT_SIZE,
  CARD_MAX_SIZE,
  CARD_MIN_SIZE,
  cardTransform,
  centreOf,
  clampSize,
  cornerOf,
  focusOf,
  handleAnchor,
  OPPOSITE_HANDLE,
  sizeOf,
} from "@/team/team-canvas-model";

const member = (id: string, parentId: string | null): TeamMember =>
  ({ id, parentId }) as TeamMember;

describe("card placement on the canvas", () => {
  it("turns a card's top-left corner into its centre and back", () => {
    const corner = { x: 40, y: -20 };
    const centre = centreOf(corner);

    expect(centre).toEqual({
      x: 40 + TEAM_CARD_WIDTH / 2,
      y: -20 + TEAM_CARD_HEIGHT / 2,
    });
    expect(cornerOf(centre)).toEqual(corner);
  });

  it("scales a corner by the zoom and then moves it by the pan", () => {
    expect(cardTransform({ x: 100, y: 50 }, 2, { x: 10, y: -5 })).toBe(
      "translate(210px, 95px) scale(2)",
    );
  });
});

describe("the focus of a selected member", () => {
  const members = [
    member("lead", null),
    member("builder", "lead"),
    member("tester", "builder"),
    member("reviewer", null),
  ];

  it("is null with no selection", () => {
    expect(focusOf(members, [], null)).toBeNull();
  });

  it("holds the member, the members above it and its pair partners", () => {
    const pairs = [{ from: "tester", to: "reviewer" }];

    expect([...(focusOf(members, pairs, "tester") ?? [])].sort()).toEqual([
      "builder",
      "lead",
      "reviewer",
      "tester",
    ]);
  });
});

describe("the handles of a card", () => {
  it("land on the opposite side of the one a line leaves from", () => {
    expect(OPPOSITE_HANDLE.bottom).toBe("top");
    expect(OPPOSITE_HANDLE.top).toBe("bottom");
    expect(OPPOSITE_HANDLE.left).toBe("right");
    expect(OPPOSITE_HANDLE.right).toBe("left");
  });

  it("sit at the middle of their side of the card", () => {
    const corner = { x: 10, y: 20 };
    expect(handleAnchor(corner, "top")).toEqual({
      x: 10 + TEAM_CARD_WIDTH / 2,
      y: 20,
    });
    expect(handleAnchor(corner, "right")).toEqual({
      x: 10 + TEAM_CARD_WIDTH,
      y: 20 + TEAM_CARD_HEIGHT / 2,
    });
  });
});

describe("a card's size", () => {
  it("is held between the smallest and the largest a card may be", () => {
    expect(clampSize({ width: 10, height: 10 })).toEqual(CARD_MIN_SIZE);
    expect(clampSize({ width: 9999, height: 9999 })).toEqual(CARD_MAX_SIZE);
    expect(clampSize({ width: 300.4, height: 150.6 })).toEqual({
      width: 300,
      height: 151,
    });
  });

  it("is the default for a member the user has not resized", () => {
    expect(sizeOf({}, "builder")).toEqual(CARD_DEFAULT_SIZE);
    expect(sizeOf({ builder: { width: 300, height: 200 } }, "builder")).toEqual(
      { width: 300, height: 200 },
    );
  });

  it("keeps the centre and the corner of a resized card in step", () => {
    const size = { width: 300, height: 200 };
    const corner = { x: 10, y: 20 };
    expect(cornerOf(centreOf(corner, size), size)).toEqual(corner);
    expect(handleAnchor(corner, "right", size)).toEqual({ x: 310, y: 120 });
  });
});

describe("open card view", () => {
  const area = { width: 1000, height: 800 };

  it("builds a card's box from its centre and size", () => {
    expect(boxOf({ x: 10, y: 10 }, { width: 20, height: 10 })).toEqual({
      x1: 0,
      y1: 5,
      x2: 20,
      y2: 15,
    });
  });

  it("fits one box and keeps the zoom in range", () => {
    const view = fitView(
      [boxOf({ x: 50, y: 50 }, { width: 100, height: 100 })],
      area,
      0,
      {
        min: 0.2,
        max: 1.5,
      },
    );
    expect(view.zoom).toBe(1.5);
    expect(view.pan).toEqual({ x: 425, y: 325 });
  });

  it("fits a wide box inside the padding", () => {
    const wide = boxOf({ x: 1000, y: 50 }, { width: 2000, height: 100 });
    const view = fitView([wide], area, 40, { min: 0.2, max: 1.5 });
    expect(view.zoom).toBeCloseTo(0.46);
    expect(wide.x1 * view.zoom + view.pan.x).toBeCloseTo(40);
    expect(wide.x2 * view.zoom + view.pan.x).toBeCloseTo(960);
  });

  it("tells whether a box is on the screen, at all", () => {
    const near = { x1: 0, y1: 0, x2: 100, y2: 100 };
    expect(isInView(near, 1, { x: 0, y: 0 }, area)).toBe(true);
    expect(isInView(near, 1, { x: -200, y: 0 }, area)).toBe(false);
    expect(
      isInView({ x1: 950, y1: 0, x2: 1050, y2: 100 }, 1, { x: 0, y: 0 }, area),
    ).toBe(true);
    expect(
      isInView({ x1: 2000, y1: 0, x2: 2100, y2: 100 }, 1, { x: 0, y: 0 }, area),
    ).toBe(false);
  });
});
