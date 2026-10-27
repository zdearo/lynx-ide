import { describe, expect, it } from "vitest";

import {
  TEAM_MEMBERS,
  TEAM_ROOT_ID,
  type TeamMember,
} from "@/team/team-fixtures";
import {
  TEAM_CARD_HEIGHT,
  TEAM_CARD_WIDTH,
  teamLayout,
  type TeamPosition,
} from "@/team/team-layout";

const centre = (position: TeamPosition) => position.x + TEAM_CARD_WIDTH / 2;

function at(positions: Map<string, TeamPosition>, id: string): TeamPosition {
  const position = positions.get(id);
  if (!position) throw new Error(`${id} is not placed`);
  return position;
}

const childrenOf = (id: string) =>
  TEAM_MEMBERS.filter((member) => member.parentId === id).map(
    (member) => member.id,
  );

// Builder gets one member of its own, to check that a subtree centres on its parent too.
const grandchild: TeamMember = {
  ...TEAM_MEMBERS[1],
  id: "grandchild",
  parentId: "builder",
};

describe("teamLayout", () => {
  it("centres the coordinator over its members, which sit side by side on the row below", () => {
    const positions = teamLayout(TEAM_MEMBERS, [TEAM_ROOT_ID]);
    const lead = at(positions, TEAM_ROOT_ID);
    const kids = childrenOf(TEAM_ROOT_ID).map((id) => at(positions, id));
    const xs = kids.map((kid) => centre(kid));
    expect(centre(lead)).toBeCloseTo((Math.min(...xs) + Math.max(...xs)) / 2);
    expect(new Set(kids.map((kid) => kid.y)).size).toBe(1);
    expect(kids[0].y).toBeGreaterThanOrEqual(lead.y + TEAM_CARD_HEIGHT);
  });

  it("never overlaps two cards", () => {
    const positions = [...teamLayout(TEAM_MEMBERS, [TEAM_ROOT_ID]).values()];
    for (const [i, a] of positions.entries()) {
      for (const b of positions.slice(i + 1)) {
        const apart =
          Math.abs(a.x - b.x) >= TEAM_CARD_WIDTH ||
          Math.abs(a.y - b.y) >= TEAM_CARD_HEIGHT;
        expect(apart).toBe(true);
      }
    }
  });

  it("hides a collapsed member's own members, and centres a deeper level too", () => {
    const members = [...TEAM_MEMBERS, grandchild];
    expect([...teamLayout(members, [])].map(([id]) => id)).toEqual([
      TEAM_ROOT_ID,
    ]);

    const positions = teamLayout(members, [TEAM_ROOT_ID]);
    expect(positions.has("grandchild")).toBe(false);

    const open = teamLayout(members, [TEAM_ROOT_ID, "builder"]);
    expect(centre(at(open, "grandchild"))).toBeCloseTo(
      centre(at(open, "builder")),
    );
  });

  it("does not place a member whose parent is not in the run", () => {
    const orphan: TeamMember = {
      ...TEAM_MEMBERS[1],
      id: "orphan",
      parentId: "gone",
    };
    const positions = teamLayout([...TEAM_MEMBERS, orphan], [TEAM_ROOT_ID]);
    expect(positions.has("orphan")).toBe(false);
  });
});
