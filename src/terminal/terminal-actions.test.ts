import { beforeEach, describe, expect, it, vi } from "vitest";

import { commands, type AppConfig, type ConfigSnapshot } from "@/bindings";
import { DEFAULT_TERMINAL_CONFIG, useConfigStore } from "@/config/config-store";
import { TERMINAL_PRESETS, type TerminalPreset } from "@/agents/agent-presets";
import { startPreset } from "@/terminal/terminal-actions";
import { session } from "@/test-support";

vi.mock("@/bindings", () => ({
  commands: {
    createTerminal: vi.fn(),
    getConfig: vi.fn(),
    updateConfig: vi.fn(),
    agentLaunchLine: vi.fn(),
  },
}));
// The team store reads the browser at load, which this file does not run in; the actions only need its view mode.
vi.mock("@/team/team-store", () => ({
  useTeamStore: { getState: () => ({ setMode: vi.fn() }) },
}));

const api = vi.mocked(commands);

function preset(id: string): TerminalPreset {
  const found = TERMINAL_PRESETS.find((candidate) => candidate.id === id);
  if (!found) throw new Error(`no preset ${id}`);
  return found;
}

function withAgents(agents: AppConfig["agents"]) {
  const snapshot: ConfigSnapshot = {
    config: {
      terminal: DEFAULT_TERMINAL_CONFIG,
      worktrees: { directory: null },
      agents,
      notifications: { onDone: true, onNeedsYou: true, onUpdate: true },
      updates: { checkAutomatically: true },
      bots: {
        chatgpt: { enabled: false },
        decider: { enabled: false },
        memory: { capture: true },
      },
      browser: { agentAccess: "full", allowedSites: [] },
      usage: {
        visible: true,
        providers: ["claude", "codex"],
        showNames: true,
        display: "used",
        layout: "compact",
        warnAt: 75,
        criticalAt: 90,
      },
      git: {
        commitMessageAgent: "auto",
        commitMessageStyle: "conventional",
        commitMessageInstructions: null,
      },
      orchestration: {
        enabled: false,
        releaseFinishedWorkers: true,
      },
    },
    issues: [],
    path: "/home/dev/.lynx/config.json",
    fileExists: true,
    fileBroken: false,
  };
  useConfigStore.getState().apply(snapshot);
}

/** The command a new terminal was asked to start with. */
async function startedWith(id: string): Promise<string | null> {
  api.createTerminal.mockResolvedValue({
    status: "ok",
    data: session("t1", "/w/one"),
  } as never);
  await startPreset("/w/one", preset(id));
  return api.createTerminal.mock.calls.at(-1)?.[1] ?? null;
}

describe("startPreset", () => {
  beforeEach(() => {
    api.createTerminal.mockReset();
    api.agentLaunchLine.mockReset();
  });

  it("starts an agent with the line the backend builds for its settings", async () => {
    withAgents({
      launchMode: "yolo",
      launchArguments: { claude: "--model opus" },
    });
    api.agentLaunchLine.mockResolvedValue(
      "claude --dangerously-skip-permissions --model opus",
    );

    expect(await startedWith("claude")).toBe(
      "claude --dangerously-skip-permissions --model opus",
    );
    expect(api.agentLaunchLine).toHaveBeenCalledWith("claude");
  });

  it("starts a plain shell with no command, without asking the backend", async () => {
    expect(await startedWith("shell")).toBeNull();
    expect(api.agentLaunchLine).not.toHaveBeenCalled();
  });
});
