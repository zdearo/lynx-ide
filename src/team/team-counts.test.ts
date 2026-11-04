import { describe, expect, it } from "vitest";

import type { TeamMember } from "@/team/team-fixtures";
import { teamCounts } from "@/team/team-counts";

const member = (
  id: string,
  parentId: string | null,
  status: TeamMember["status"] = "idle",
): TeamMember => ({
  id,
  kind: "agent",
  role: id,
  harness: "claude",
  sessionId: null,
  status,
  worktree: "/work/auth",
  parentId,
  orchestrator: false,
  task: "",
  acceptance: "",
  activity: { text: "", at: 0 },
  branch: null,
  added: 0,
  removed: 0,
  waitingOn: null,
  note: null,
  question: null,
  output: [],
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
