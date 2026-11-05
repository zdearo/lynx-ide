import { beforeEach, describe, expect, it, vi } from "vitest";

import { commands } from "@/bindings";
import { teamModeOf, useTeamStore } from "@/team/team-store";
import { startTerminal } from "@/terminal/terminal-actions";
import { useTerminalStore } from "@/terminal/terminal-store";
import { ok, session } from "@/test-support";
import { useWorkspaceStore } from "@/workspace/workspace-store";

vi.mock("@/bindings", () => ({
  commands: {
    createTerminal: vi.fn(),
    closeTerminal: vi.fn(),
  },
}));
vi.mock("@/dev/mock-mode", () => ({ isMockBackend: () => false }));

const api = vi.mocked(commands);
const WT = "/work/api";
const mode = () => teamModeOf(useTeamStore.getState().modeByWorktree, WT);

beforeEach(() => {
  vi.resetAllMocks();
  useTerminalStore.setState({ sessions: [], activeByWorktree: {} });
  useTeamStore.setState({ modeByWorktree: { [WT]: "team" } });
  useWorkspaceStore.setState({ selectedWorktreePath: WT });
});

describe("the central view", () => {
  it("stays on the team when the backend starts a session, such as a worker", () => {
    useTerminalStore.getState().added(session("worker", WT));

    expect(mode()).toBe("team");
  });

  it("stays on the team when the team starts a member's agent", async () => {
    api.createTerminal.mockResolvedValue(ok(session("agent", WT)));

    await useTerminalStore.getState().create(WT, "claude");

    expect(mode()).toBe("team");
  });

  it("shows the terminals when the person starts one in the worktree", async () => {
    api.createTerminal.mockResolvedValue(ok(session("t1", WT)));

    await expect(startTerminal(WT, null)).resolves.toBe(true);

    expect(mode()).toBe("terminal");
  });
});
