import { describe, expect, it } from "vitest";

import type { TeamMember } from "@/team/team-fixtures";
import { TEAM_CARD_HEIGHT, TEAM_CARD_WIDTH } from "@/team/team-layout";
import {
  cardTransform,
  centreOf,
  cornerOf,
  focusOf,
  handleAnchor,
  OPPOSITE_HANDLE,
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
