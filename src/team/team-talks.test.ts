import { describe, expect, it } from "vitest";

import type { TeamAgent, TeamMember } from "@/team/team-fixtures";
import type { TeamPair } from "@/team/team-pairs";
import { talksToOf } from "@/team/team-talks";

const base = {
  worktree: "/work/auth",
  status: "idle" as const,
  workerState: null,
  parentId: null as string | null,
  orchestrator: false,
  task: "",
  acceptance: "",
  taskState: null,
  dependsOn: [],
  executionWorktree: null,
  activity: { text: "", at: 0 },
  branch: null,
  added: 0,
  removed: 0,
  waitingOn: null,
  note: null,
  question: null,
  output: [] as string[],
};

/** An agent member; `sessionId` names its terminal session, or null for none. */
const agent = (
  id: string,
  role: string,
  overrides: Partial<TeamAgent> = {},
): TeamAgent => ({
  ...base,
  id,
  role,
  kind: "agent",
  harness: "claude",
  sessionId: null,
  ...overrides,
});

const bot = (id: string, role: string): TeamMember => ({
  ...base,
  id,
  role,
  kind: "bot",
  botId: id,
  orchestrator: true,
});

const lead = agent("lead", "Lead", { orchestrator: true });
const builder = agent("builder", "Builder", {
  parentId: "lead",
  orchestrator: true,
});
const reviewer = agent("reviewer", "Reviewer");
const scout = agent("scout", "Scout", { parentId: "lead" });
const stopped = agent("stopped", "Stopped", { parentId: "lead" });
const docs = bot("docs", "Docs");
const loner = agent("loner", "Loner");
const members = [lead, builder, reviewer, scout, stopped, docs, loner];
// The builder is paired with the reviewer, the scout, the stopped agent and the docs bot.
const pairs: TeamPair[] = [
  { from: "builder", to: "reviewer" },
  { from: "builder", to: "scout" },
  { from: "builder", to: "stopped" },
  { from: "builder", to: "docs" },
];

describe("talks to", () => {
  it("is the pairs, the orchestrator above, and the members under it", () => {
    expect(talksToOf(builder, members, pairs).map((m) => m.id)).toEqual([
      "lead",
      "reviewer",
      "scout",
      "stopped",
      "docs",
    ]);
    expect(talksToOf(lead, members, pairs).map((m) => m.id)).toEqual([
      "builder",
      "scout",
      "stopped",
    ]);
  });

  it("is empty for a member with no pair, parent or child", () => {
    expect(talksToOf(loner, members, pairs)).toEqual([]);
  });
});
