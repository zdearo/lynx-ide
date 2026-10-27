import { describe, expect, it } from "vitest";

import type { TeamMember } from "@/team/team-fixtures";
import { teamCounts, teamRows } from "@/team/team-rows";

const member = (
  id: string,
  parentId: string | null,
  status: TeamMember["status"] = "idle",
): TeamMember => ({
  id,
  role: id,
  harness: "claude",
  status,
  parentId,
  task: "",
  acceptance: "",
  activity: { text: "", at: 0 },
  branch: null,
  added: 0,
  removed: 0,
  pairedWith: null,
  waitingOn: null,
  note: null,
  question: null,
  output: [],
});

// a has b under it, b has c; d is a second root; "gone" is a parent that is not in the list.
const members = [
  member("a", null),
  member("b", "a"),
  member("c", "b"),
  member("d", null),
  member("orphan", "gone"),
];

describe("teamRows", () => {
  it("shows only the roots while nothing is expanded", () => {
    const rows = teamRows(members, []);
    expect(rows.map((row) => row.member.id)).toEqual(["a", "d"]);
    expect(rows[0]).toMatchObject({ hasChildren: true, expanded: false });
    expect(rows[1]).toMatchObject({ hasChildren: false, expanded: false });
  });

  it("lists an expanded member's children one level deeper", () => {
    const rows = teamRows(members, ["a"]);
    expect(rows.map((row) => [row.member.id, row.depth])).toEqual([
      ["a", 0],
      ["b", 1],
      ["d", 0],
    ]);
    expect(rows[1]).toMatchObject({ hasChildren: true, expanded: false });
  });

  it("walks down through every expanded level", () => {
    const rows = teamRows(members, ["a", "b"]);
    expect(rows.map((row) => [row.member.id, row.depth])).toEqual([
      ["a", 0],
      ["b", 1],
      ["c", 2],
      ["d", 0],
    ]);
  });

  it("never shows a member whose parent is not in the run", () => {
    const rows = teamRows(members, ["a", "b", "gone"]);
    expect(rows.some((row) => row.member.id === "orphan")).toBe(false);
  });
});

describe("teamCounts", () => {
  it("counts working and needs-you members", () => {
    const counts = teamCounts([
      member("a", null, "working"),
      member("b", "a", "working"),
      member("c", "a", "needsYou"),
      member("d", "a", "done"),
    ]);
    expect(counts).toEqual({ working: 2, needsYou: 1 });
  });
});
