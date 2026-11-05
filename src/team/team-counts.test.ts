import { describe, expect, it } from "vitest";

import type { TerminalInfo } from "@/bindings";
import type { TeamMember } from "@/team/team-fixtures";
import { teamCounts } from "@/team/team-counts";

const member = (
  id: string,
  status: TeamMember["status"] = "idle",
  sessionId: string | null = null,
): TeamMember => ({
  id,
  kind: "agent",
  role: id,
  harness: "claude",
  sessionId,
  status,
  workerState: null,
  worktree: "/work/auth",
  parentId: null,
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
  output: [],
});

const session = (id: string, running = true): TerminalInfo => ({
  id,
  worktreePath: "/work/auth",
  command: "claude",
  shell: "zsh",
  running,
  hibernated: false,
  exitCode: null,
  startedAt: 0,
  harness: "claude",
  agentStatus: "idle",
});

describe("teamCounts", () => {
  it("counts working and needs-you members", () => {
    const counts = teamCounts(
      [
        member("a", "working"),
        member("b", "working"),
        member("c", "needsYou"),
        member("d", "done"),
      ],
      [],
    );
    expect(counts).toEqual({ working: 2, needsYou: 1, running: 0 });
  });

  it("counts a live agent that waits as running", () => {
    const counts = teamCounts([member("a", "idle", "s-a")], [session("s-a")]);
    expect(counts).toEqual({ working: 0, needsYou: 0, running: 1 });
  });

  it("does not count a live agent that works or needs the user as running", () => {
    const counts = teamCounts(
      [member("a", "working", "s-a"), member("b", "needsYou", "s-b")],
      [session("s-a"), session("s-b")],
    );
    expect(counts).toEqual({ working: 1, needsYou: 1, running: 0 });
  });

  it("does not count a done or closed member as running, even with a session", () => {
    const counts = teamCounts(
      [member("a", "done", "s-a"), member("b", "closed", "s-b")],
      [session("s-a"), session("s-b")],
    );
    expect(counts.running).toBe(0);
  });

  it("does not count a member with no session, or a stopped one", () => {
    const counts = teamCounts(
      [member("a", "idle"), member("b", "idle", "s-b")],
      [session("s-b", false)],
    );
    expect(counts.running).toBe(0);
  });
});
