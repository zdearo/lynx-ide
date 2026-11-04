import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { toast } from "sonner";

import { commands, type TerminalInfo } from "@/bindings";
import { IpcRequestError } from "@/lib/ipc-result";
import type { TeamAgent, TeamMember } from "@/team/team-fixtures";
import type { TeamPair } from "@/team/team-pairs";
import {
  canStartConversation,
  MESSAGE_MAX_LENGTH,
  messageRecipients,
  sendTeamMessage,
  showsMessageForm,
} from "@/team/team-message";
import { copy } from "@/ui-copy";

vi.mock("sonner", () => ({ toast: { error: vi.fn() } }));
const terminal = vi.hoisted(() => ({ woken: vi.fn() }));
vi.mock("@/terminal/terminal-store", () => ({
  useTerminalStore: { getState: () => terminal },
}));

const base = {
  worktree: "/work/auth",
  status: "idle" as const,
  parentId: null as string | null,
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

const lead = agent("lead", "Lead", { orchestrator: true, sessionId: "s-lead" });
const builder = agent("builder", "Builder", {
  parentId: "lead",
  orchestrator: true,
  sessionId: "s-builder",
});
const reviewer = agent("reviewer", "Reviewer", { sessionId: "s-reviewer" });
const scout = agent("scout", "Scout", { parentId: "lead" });
const stopped = agent("stopped", "Stopped", {
  parentId: "lead",
  sessionId: "s-stopped",
});
const docs = bot("docs", "Docs");
const loner = agent("loner", "Loner", { sessionId: "s-loner" });
const members = [lead, builder, reviewer, scout, stopped, docs, loner];
// The builder is paired with everyone it could talk to; only the reviewer has a live session among them.
const pairs: TeamPair[] = [
  { from: "builder", to: "reviewer" },
  { from: "builder", to: "scout" },
  { from: "builder", to: "stopped" },
  { from: "builder", to: "docs" },
];
const sessions = [
  session("s-lead"),
  session("s-builder"),
  session("s-reviewer"),
  session("s-stopped", false),
  session("s-loner"),
];

beforeEach(() => {
  vi.resetAllMocks();
});

afterEach(() => {
  vi.restoreAllMocks();
});

describe("who can message", () => {
  it("gives a loose member no form, because it only answers", () => {
    expect(canStartConversation(reviewer)).toBe(false);
    expect(showsMessageForm(reviewer, sessions)).toBe(false);
    expect(showsMessageForm(loner, sessions)).toBe(false);
  });

  it("gives the form to an orchestrator, or a member under one, with a running session", () => {
    expect(showsMessageForm(lead, sessions)).toBe(true);
    expect(showsMessageForm(builder, sessions)).toBe(true);
  });

  it("gives no form to a member with no live session", () => {
    expect(showsMessageForm(scout, sessions)).toBe(false);
    expect(showsMessageForm(stopped, sessions)).toBe(false);
    expect(showsMessageForm(docs, sessions)).toBe(false);
  });
});

describe("recipients", () => {
  it("offers only the paired members whose sessions run", () => {
    expect(messageRecipients(builder, members, pairs, sessions)).toEqual([
      { id: "reviewer", role: "Reviewer", sessionId: "s-reviewer" },
    ]);
  });

  it("offers nobody when no paired member has a live session", () => {
    expect(messageRecipients(lead, members, pairs, sessions)).toEqual([]);
  });
});

describe("sending", () => {
  it("writes the text with the sender's role and a newline into the recipient's terminal", async () => {
    vi.spyOn(commands, "writeTerminal").mockResolvedValue({
      status: "ok",
      data: null,
    });

    const sent = await sendTeamMessage(
      "Builder",
      "s-reviewer",
      "check the login",
    );

    expect(sent).toBe(true);
    expect(commands.writeTerminal).toHaveBeenCalledWith(
      "s-reviewer",
      "[from Builder] check the login\r",
    );
    expect(terminal.woken).toHaveBeenCalledWith("s-reviewer");
  });

  it("writes nothing for an empty, blank or too-long message", async () => {
    const write = vi.spyOn(commands, "writeTerminal");

    expect(await sendTeamMessage("Builder", "s-reviewer", "")).toBe(false);
    expect(await sendTeamMessage("Builder", "s-reviewer", "  \t ")).toBe(false);
    expect(
      await sendTeamMessage(
        "Builder",
        "s-reviewer",
        "x".repeat(MESSAGE_MAX_LENGTH + 1),
      ),
    ).toBe(false);

    expect(write).not.toHaveBeenCalled();
  });

  it("sends a message of exactly the limit", async () => {
    vi.spyOn(commands, "writeTerminal").mockResolvedValue({
      status: "ok",
      data: null,
    });

    expect(
      await sendTeamMessage(
        "Builder",
        "s-reviewer",
        "x".repeat(MESSAGE_MAX_LENGTH),
      ),
    ).toBe(true);
  });

  it("shows a failed write with the backend's message, and reports it so the text is kept", async () => {
    vi.spyOn(commands, "writeTerminal").mockResolvedValue({
      status: "error",
      error: {
        kind: "terminalNotRunning",
        message: "the terminal is not running",
      },
    });

    expect(
      await sendTeamMessage("Builder", "s-reviewer", "check the login"),
    ).toBe(false);

    expect(toast.error).toHaveBeenCalledWith(copy.team.message.sendFailed, {
      description: "the terminal is not running",
    });
    expect(terminal.woken).not.toHaveBeenCalled();
  });

  it("shows a write that rejects, and reports it as not sent", async () => {
    vi.spyOn(commands, "writeTerminal").mockRejectedValue(
      new IpcRequestError({ kind: "terminal", message: "the pipe broke" }),
    );

    expect(await sendTeamMessage("Builder", "s-reviewer", "hi")).toBe(false);
    expect(toast.error).toHaveBeenCalledWith(copy.team.message.sendFailed, {
      description: "the pipe broke",
    });
  });
});
