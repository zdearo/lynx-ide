import { describe, expect, it } from "vitest";

import {
  TEAM_DOCS_MEMBERS,
  TEAM_MEMBERS,
  TEAM_PAIRS,
  TEAM_ROOT_ID,
  type TeamMember,
} from "@/team/team-fixtures";
import {
  TEAM_CARD_HEIGHT,
  TEAM_CARD_WIDTH,
  TEAM_TREE_GAP,
  TOP_LEVEL,
  extraRowsAt,
  siblingOrderAt,
  teamLayout,
  type TeamPosition,
} from "@/team/team-layout";

const NO_EXTRA_ROWS: Record<string, number> = {};
const ROW = TEAM_CARD_HEIGHT + 64;

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

/** The lead's tree without the Reviewer, which has no partner here and so stands apart from it. */
const TREE = TEAM_MEMBERS.filter((member) => member.id !== "reviewer");
const TREE_IDS = TREE.map((member) => member.id);
const DOCS_IDS = TEAM_DOCS_MEMBERS.map((member) => member.id);

/** The left edge and the right edge of the placed cards with these ids; a collapsed one is not placed. */
function edgesOf(positions: Map<string, TeamPosition>, ids: readonly string[]) {
  const spots = ids.flatMap((id) => {
    const spot = positions.get(id);
    return spot ? [spot] : [];
  });
  return {
    left: Math.min(...spots.map((spot) => spot.x)),
    right: Math.max(...spots.map((spot) => spot.x + TEAM_CARD_WIDTH)),
  };
}

describe("teamLayout", () => {
  it("centres the coordinator over its members, which sit side by side on the row below", () => {
    const positions = teamLayout(TEAM_MEMBERS, [TEAM_ROOT_ID], NO_EXTRA_ROWS);
    const lead = at(positions, TEAM_ROOT_ID);
    const kids = childrenOf(TEAM_ROOT_ID).map((id) => at(positions, id));
    const xs = kids.map((kid) => centre(kid));
    expect(centre(lead)).toBeCloseTo((Math.min(...xs) + Math.max(...xs)) / 2);
    expect(new Set(kids.map((kid) => kid.y)).size).toBe(1);
    expect(kids[0].y).toBeGreaterThanOrEqual(lead.y + TEAM_CARD_HEIGHT);
  });

  it("never overlaps two cards", () => {
    const positions = [
      ...teamLayout(TEAM_MEMBERS, [TEAM_ROOT_ID], NO_EXTRA_ROWS).values(),
    ];
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
    // A member with no orchestrator is a root too, so it stays on the top row whatever is collapsed.
    expect([...teamLayout(TEAM_MEMBERS, [], NO_EXTRA_ROWS).keys()]).toEqual([
      TEAM_ROOT_ID,
      "reviewer",
    ]);

    const closed = teamLayout(TEAM_MEMBERS, [TEAM_ROOT_ID], NO_EXTRA_ROWS);
    expect(closed.has("login-tests")).toBe(false);

    const open = teamLayout(
      TEAM_MEMBERS,
      [TEAM_ROOT_ID, "builder"],
      NO_EXTRA_ROWS,
    );
    expect(centre(at(open, "login-tests"))).toBeCloseTo(
      centre(at(open, "builder")),
    );
  });

  it("does not place a member whose parent is not in the team", () => {
    const orphan: TeamMember = {
      ...TEAM_MEMBERS[1],
      id: "orphan",
      parentId: "gone",
    };
    const positions = teamLayout(
      [...TEAM_MEMBERS, orphan],
      [TEAM_ROOT_ID],
      NO_EXTRA_ROWS,
    );
    expect(positions.has("orphan")).toBe(false);
  });

  it("moves a member down by its extra rows, and its own members with it", () => {
    const open = [TEAM_ROOT_ID, "builder"];
    const flat = teamLayout(TEAM_MEMBERS, open, NO_EXTRA_ROWS);
    const moved = teamLayout(TEAM_MEMBERS, open, { builder: 2 });
    expect(at(moved, "builder").y - at(flat, "builder").y).toBeCloseTo(2 * ROW);
    expect(at(moved, "login-tests").y - at(flat, "login-tests").y).toBeCloseTo(
      2 * ROW,
    );
    // Alone on its row, the branch is centred under the coordinator, its own member under it.
    expect(centre(at(moved, "builder"))).toBeCloseTo(
      centre(at(moved, TEAM_ROOT_ID)),
    );
    expect(centre(at(moved, "login-tests"))).toBeCloseTo(
      centre(at(moved, "builder")),
    );
  });

  it("closes the gap a member leaves when it moves to another row", () => {
    const flat = teamLayout(TREE, [TEAM_ROOT_ID], NO_EXTRA_ROWS);
    const moved = teamLayout(TREE, [TEAM_ROOT_ID], { researcher: 1 });
    expect(at(moved, "researcher").y).toBe(2 * ROW);

    // The ones left on the row under the coordinator sit side by side, centred under it.
    const row = childrenOf(TEAM_ROOT_ID).filter((id) => id !== "researcher");
    const stayed = row.map((id) => at(moved, id)).sort((a, b) => a.x - b.x);
    for (const [i, card] of stayed.slice(1).entries()) {
      expect(card.x - stayed[i].x).toBe(TEAM_CARD_WIDTH + 32);
    }
    const xs = stayed.map(centre);
    expect((Math.min(...xs) + Math.max(...xs)) / 2).toBeCloseTo(
      centre(at(moved, TEAM_ROOT_ID)),
    );
    // And they take less room than they did with the moved one between them.
    const spread = (positions: Map<string, TeamPosition>) =>
      Math.max(...row.map((id) => at(positions, id).x)) -
      Math.min(...row.map((id) => at(positions, id).x));
    expect(spread(moved)).toBeLessThan(spread(flat));
  });

  it("puts a moved member under its parent when nothing else is on its row", () => {
    const moved = teamLayout(TEAM_MEMBERS, [TEAM_ROOT_ID], { researcher: 1 });
    expect(centre(at(moved, "researcher"))).toBeCloseTo(
      centre(at(moved, TEAM_ROOT_ID)),
    );
  });

  it("never overlaps two cards, however the members are moved", () => {
    const moves: Record<string, number>[] = [
      { researcher: 1 },
      { builder: 1, researcher: 2 },
      { researcher: 3, tester: 1 },
      { builder: 1, tester: 1, researcher: 1 },
    ];
    for (const extraRows of moves) {
      for (const open of [[TEAM_ROOT_ID], [TEAM_ROOT_ID, "builder"]]) {
        const positions = [
          ...teamLayout(TEAM_MEMBERS, open, extraRows).values(),
        ];
        for (const [i, a] of positions.entries()) {
          for (const b of positions.slice(i + 1)) {
            const apart =
              Math.abs(a.x - b.x) >= TEAM_CARD_WIDTH ||
              Math.abs(a.y - b.y) >= TEAM_CARD_HEIGHT;
            expect(apart, JSON.stringify({ extraRows, open })).toBe(true);
          }
        }
      }
    }
  });

  it("never lifts a member above the row under its parent, nor the coordinator off the top", () => {
    const flat = teamLayout(TEAM_MEMBERS, [TEAM_ROOT_ID], NO_EXTRA_ROWS);
    const lifted = teamLayout(TEAM_MEMBERS, [TEAM_ROOT_ID], {
      builder: -3,
      lead: 4,
    });
    expect(at(lifted, "builder")).toEqual(at(flat, "builder"));
    expect(at(lifted, TEAM_ROOT_ID).y).toBe(0);
  });

  it("keeps the first tree's root on x = 0, so the map is not shifted", () => {
    const positions = teamLayout(
      [...TREE, ...TEAM_DOCS_MEMBERS],
      [TEAM_ROOT_ID, "docs-lead"],
      NO_EXTRA_ROWS,
    );
    expect(centre(at(positions, TEAM_ROOT_ID))).toBe(0);
  });

  it("gives a single tree the positions it had before trees were laid out apart", () => {
    const expanded = [TEAM_ROOT_ID, "builder"];
    const pairs = TEAM_PAIRS.map(([from, to]) => ({ from, to }));
    const asPairs = (positions: Map<string, TeamPosition>) =>
      Object.fromEntries([...positions].map(([id, { x, y }]) => [id, [x, y]]));

    expect(asPairs(teamLayout(TREE, expanded, NO_EXTRA_ROWS))).toEqual({
      lead: [-112, 0],
      builder: [-368, 176],
      researcher: [-112, 176],
      tester: [144, 176],
      "login-tests": [-368, 352],
    });
    expect(
      asPairs(teamLayout(TREE, [TEAM_ROOT_ID], { researcher: 1 })),
    ).toEqual({
      lead: [-112, 0],
      builder: [-240, 176],
      tester: [16, 176],
      researcher: [-112, 352],
    });
    expect(
      asPairs(
        teamLayout(
          TREE,
          [TEAM_ROOT_ID],
          {},
          {
            [TEAM_ROOT_ID]: ["tester", "builder", "researcher"],
          },
        ),
      ),
    ).toEqual({
      lead: [-112, 0],
      tester: [-368, 176],
      builder: [-112, 176],
      researcher: [144, 176],
    });
    // The Reviewer is paired with Builder, so it joins the tree and sits beside it.
    expect(
      asPairs(teamLayout(TEAM_MEMBERS, expanded, NO_EXTRA_ROWS, {}, pairs)),
    ).toEqual({
      lead: [-112, 0],
      builder: [-560, 176],
      reviewer: [-304, 176],
      researcher: [-48, 176],
      tester: [208, 176],
      "login-tests": [-560, 352],
    });
  });

  it("lays out each tree as its own block, with a gap of TEAM_TREE_GAP between two blocks", () => {
    const positions = teamLayout(
      [...TREE, ...TEAM_DOCS_MEMBERS],
      [TEAM_ROOT_ID, "docs-lead"],
      NO_EXTRA_ROWS,
    );
    const lead = edgesOf(positions, TREE_IDS);
    const docs = edgesOf(positions, DOCS_IDS);
    expect(docs.left - lead.right).toBeGreaterThanOrEqual(TEAM_TREE_GAP);
    // The Writer sits under the bot Lead, so the bot's block is centred on its own root.
    expect(centre(at(positions, "writer"))).toBe(
      centre(at(positions, "docs-lead")),
    );
  });

  it("keeps two trees apart, however the members are moved", () => {
    const members = [...TREE, ...TEAM_DOCS_MEMBERS];
    const expanded = [TEAM_ROOT_ID, "builder", "docs-lead"];
    for (const extraRows of [
      NO_EXTRA_ROWS,
      { researcher: 2 },
      { builder: 1, writer: 2 },
    ]) {
      const positions = teamLayout(members, expanded, extraRows);
      const lead = edgesOf(positions, TREE_IDS);
      const docs = edgesOf(positions, DOCS_IDS);
      expect(
        docs.left - lead.right,
        JSON.stringify(extraRows),
      ).toBeGreaterThanOrEqual(TEAM_TREE_GAP);
    }
  });

  it("takes the order of the trees from the saved top level, then from reading order", () => {
    const positions = teamLayout(
      [...TREE, ...TEAM_DOCS_MEMBERS],
      [TEAM_ROOT_ID, "docs-lead"],
      NO_EXTRA_ROWS,
      { [TOP_LEVEL]: ["docs-lead", TEAM_ROOT_ID] },
    );
    // The docs tree is first now, so it takes the place on x = 0 and the lead's tree sits after it.
    expect(centre(at(positions, "docs-lead"))).toBe(0);
    const docs = edgesOf(positions, DOCS_IDS);
    const lead = edgesOf(positions, TREE_IDS);
    expect(lead.left - docs.right).toBeGreaterThanOrEqual(TEAM_TREE_GAP);
  });

  it("puts a member with no orchestrator and no partner in a last block, one row below the top", () => {
    const positions = teamLayout(TEAM_MEMBERS, [TEAM_ROOT_ID], NO_EXTRA_ROWS);
    const lead = edgesOf(positions, TREE_IDS);
    const reviewer = at(positions, "reviewer");
    expect(reviewer.y).toBe(ROW);
    expect(reviewer.x - lead.right).toBeGreaterThanOrEqual(TEAM_TREE_GAP);
  });

  it("puts a loose member that is paired with a member of another tree inside that tree's block", () => {
    const scout: TeamMember = {
      ...TEAM_MEMBERS[3],
      id: "scout",
      parentId: null,
      orchestrator: false,
    };
    const members = [...TREE, ...TEAM_DOCS_MEMBERS, scout];
    const positions = teamLayout(
      members,
      [TEAM_ROOT_ID, "docs-lead"],
      NO_EXTRA_ROWS,
      {},
      [{ from: "scout", to: "writer" }],
    );
    const lead = edgesOf(positions, TREE_IDS);
    const scoutSpot = at(positions, "scout");
    // Beside the Writer, and not a whole gap away from the docs tree: it is part of that block.
    expect(scoutSpot.y).toBe(at(positions, "writer").y);
    expect(scoutSpot.x).toBeGreaterThanOrEqual(lead.right + TEAM_TREE_GAP);
    expect(scoutSpot.x - edgesOf(positions, DOCS_IDS).right).toBeLessThan(
      TEAM_TREE_GAP,
    );
  });
});

describe("extraRowsAt", () => {
  it("turns a dropped height into the nearest row under the parent, never above it", () => {
    const positions = teamLayout(TEAM_MEMBERS, [TEAM_ROOT_ID], NO_EXTRA_ROWS);
    // Builder sits under the coordinator: its default row is one below it.
    expect(extraRowsAt(TEAM_MEMBERS, positions, "builder", ROW)).toBe(0);
    expect(extraRowsAt(TEAM_MEMBERS, positions, "builder", 3.4 * ROW)).toBe(2);
    expect(extraRowsAt(TEAM_MEMBERS, positions, "builder", -ROW)).toBe(0);
  });
});

describe("sibling order", () => {
  const open = [TEAM_ROOT_ID];
  const kids = childrenOf(TEAM_ROOT_ID);

  it("lays siblings out in the saved order", () => {
    const order = { [TEAM_ROOT_ID]: [...kids].reverse() };
    const positions = teamLayout(TEAM_MEMBERS, open, NO_EXTRA_ROWS, order);
    const byX = [...kids].sort(
      (a, b) => at(positions, a).x - at(positions, b).x,
    );
    expect(byX).toEqual([...kids].reverse());
  });

  it("puts a card dropped past its neighbour after it, and keeps the order when it stays", () => {
    const positions = teamLayout(TEAM_MEMBERS, open, NO_EXTRA_ROWS);
    const [first, second] = kids;
    const next = siblingOrderAt(
      TEAM_MEMBERS,
      positions,
      first,
      at(positions, second).x + 10,
      {},
    );
    expect(next[TEAM_ROOT_ID].slice(0, 2)).toEqual([second, first]);
    const same = {};
    expect(
      siblingOrderAt(
        TEAM_MEMBERS,
        positions,
        first,
        at(positions, first).x,
        same,
      ),
    ).toBe(same);
  });
});

describe("members that answer to no one", () => {
  const loose: TeamMember = {
    ...TEAM_MEMBERS[3],
    id: "loose",
    parentId: null,
    orchestrator: false,
  };
  const pairs = [{ from: "loose", to: "tester" }];

  it("sit one row below the top when nothing is paired with them", () => {
    const positions = teamLayout(
      [...TEAM_MEMBERS, loose],
      [TEAM_ROOT_ID],
      NO_EXTRA_ROWS,
    );
    expect(at(positions, "loose").y).toBe(ROW);
  });

  it("sit on their partner's row, side by side with it", () => {
    const positions = teamLayout(
      [...TEAM_MEMBERS, loose],
      [TEAM_ROOT_ID],
      NO_EXTRA_ROWS,
      {},
      pairs,
    );
    expect(at(positions, "loose").y).toBe(at(positions, "tester").y);
    expect(
      Math.abs(at(positions, "loose").x - at(positions, "tester").x),
    ).toBeGreaterThanOrEqual(TEAM_CARD_WIDTH);
  });

  it("are never moved down", () => {
    const members = [...TEAM_MEMBERS, loose];
    const positions = teamLayout(members, [TEAM_ROOT_ID], NO_EXTRA_ROWS);
    expect(extraRowsAt(members, positions, "loose", 5 * ROW)).toBe(0);
  });

  it("go to the other side of their partner when dropped past it", () => {
    const members = [...TEAM_MEMBERS, loose];
    const positions = teamLayout(
      members,
      [TEAM_ROOT_ID],
      NO_EXTRA_ROWS,
      {},
      pairs,
    );
    const partner = at(positions, "tester");
    const left = siblingOrderAt(
      members,
      positions,
      "loose",
      partner.x - 10,
      {},
      pairs,
    );
    const moved = teamLayout(
      members,
      [TEAM_ROOT_ID],
      NO_EXTRA_ROWS,
      left,
      pairs,
    );
    expect(at(moved, "loose").x).toBeLessThan(at(moved, "tester").x);
    expect(
      siblingOrderAt(members, positions, "loose", partner.x + 10, {}, pairs),
    ).toEqual({});
  });
});
