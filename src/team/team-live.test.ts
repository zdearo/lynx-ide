import { describe, expect, it } from "vitest";

import type { TerminalInfo } from "@/bindings";
import type { TeamMember } from "@/team/team-fixtures";
import { actionKindOf, activityOf, runningLabelOf } from "@/team/team-live";
import { copy } from "@/ui-copy";

const NOW_MS = 1_700_000_000_000;
const NOW_SECONDS = NOW_MS / 1000;

const agent = (
  overrides: Partial<TeamMember> & { sessionId?: string | null } = {},
): TeamMember =>
  ({
    id: "builder",
    kind: "agent",
    role: "builder",
    harness: "claude",
    sessionId: "s-builder",
    status: "working",
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
    ...overrides,
  }) as TeamMember;

const session = (
  id: string,
  startedAt: number,
  running = true,
): TerminalInfo => ({
  id,
  worktreePath: "/work/auth",
  command: "claude",
  shell: "zsh",
  running,
  hibernated: false,
  exitCode: null,
  startedAt,
  harness: "claude",
  agentStatus: "working",
});

describe("runningLabelOf", () => {
  it("shows how long a live agent has run", () => {
    const label = runningLabelOf(
      agent(),
      [session("s-builder", NOW_SECONDS - 300)],
      NOW_MS,
    );
    expect(label).toBe(copy.team.runningFor("5m"));
  });

  it("shows the hours of a session that has run for hours", () => {
    const sessions = [session("s-builder", NOW_SECONDS - 7200)];
    expect(runningLabelOf(agent(), sessions, NOW_MS)).toBe(
      copy.team.runningFor("2h"),
    );
  });

  it("says a session that started under a minute ago has just started", () => {
    const label = runningLabelOf(
      agent(),
      [session("s-builder", NOW_SECONDS - 20)],
      NOW_MS,
    );
    expect(label).toBe(copy.team.justStarted);
  });

  it("shows nothing for a member with no session", () => {
    expect(runningLabelOf(agent({ sessionId: null }), [], NOW_MS)).toBeNull();
  });

  it("shows nothing when the session of the member is not in the list", () => {
    expect(
      runningLabelOf(agent(), [session("other", NOW_SECONDS - 300)], NOW_MS),
    ).toBeNull();
  });

  it("shows nothing when the session has ended", () => {
    expect(
      runningLabelOf(
        agent(),
        [session("s-builder", NOW_SECONDS - 300, false)],
        NOW_MS,
      ),
    ).toBeNull();
  });

  it("shows nothing for a bot, which has no session", () => {
    const bot = agent({ kind: "bot", botId: "bot-1" } as Partial<TeamMember>);
    expect(
      runningLabelOf(bot, [session("s-builder", NOW_SECONDS - 300)], NOW_MS),
    ).toBeNull();
  });

  it.each(["done", "failed", "closed"] as const)(
    "shows nothing for a %s member, even with its session still running",
    (status) => {
      expect(
        runningLabelOf(
          agent({ status }),
          [session("s-builder", NOW_SECONDS - 300)],
          NOW_MS,
        ),
      ).toBeNull();
    },
  );

  it.each(["working", "needsYou", "idle"] as const)(
    "shows the time for a live %s member",
    (status) => {
      expect(
        runningLabelOf(
          agent({ status }),
          [session("s-builder", NOW_SECONDS - 300)],
          NOW_MS,
        ),
      ).toBe(copy.team.runningFor("5m"));
    },
  );
});

describe("actionKindOf", () => {
  it.each(["done", "failed", "noReport", "needsYou", "question", "answer"])(
    "keeps the known kind %s",
    (kind) => {
      expect(actionKindOf(kind)).toBe(kind);
    },
  );

  it("maps a kind the screen does not know to other, without throwing", () => {
    expect(actionKindOf("escalated")).toBe("other");
    expect(actionKindOf("")).toBe("other");
  });
});

describe("activityOf", () => {
  it("is the newest message as the member's activity, with its kind", () => {
    expect(
      activityOf({ kind: "needsYou", body: "pick a branch", createdAt: 1_700 }),
    ).toEqual({ text: "pick a branch", at: 1_700, kind: "needsYou" });
  });

  it("is other for a message kind the screen does not know", () => {
    expect(activityOf({ kind: "escalated", body: "x", createdAt: 3 })).toEqual({
      text: "x",
      at: 3,
      kind: "other",
    });
  });

  it("is null for a member with no message", () => {
    expect(activityOf(null)).toBeNull();
  });
});
