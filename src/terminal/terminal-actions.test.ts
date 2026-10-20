import { beforeEach, describe, expect, it, vi } from "vitest";

import { commands, type AppConfig, type ConfigSnapshot } from "@/bindings";
import { DEFAULT_TERMINAL_CONFIG, useConfigStore } from "@/config/config-store";
import {
  TERMINAL_PRESETS,
  type TerminalPreset,
} from "@/terminal/agent-presets";
import { startPreset } from "@/terminal/terminal-actions";
import { session } from "@/test-support";

vi.mock("@/bindings", () => ({
  commands: {
    createTerminal: vi.fn(),
    getConfig: vi.fn(),
    updateConfig: vi.fn(),
  },
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
      notifications: { onDone: true, onNeedsYou: true },
      browser: { agentAccess: "full", allowedSites: [] },
      git: {
        commitMessageAgent: "auto",
        commitMessageStyle: "conventional",
        commitMessageInstructions: null,
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
  });

  it("starts an agent in YOLO mode with its flag and its arguments", async () => {
    withAgents({
      launchMode: "yolo",
      launchArguments: { claude: "--model opus" },
    });
    expect(await startedWith("claude")).toBe(
      "claude --dangerously-skip-permissions --model opus",
    );
  });

  it("starts an agent in manual mode with only its arguments", async () => {
    withAgents({
      launchMode: "manual",
      launchArguments: { claude: "--model opus" },
    });
    expect(await startedWith("claude")).toBe("claude --model opus");
  });

  it("starts a plain shell with no command at all", async () => {
    withAgents({ launchMode: "yolo", launchArguments: {} });
    expect(await startedWith("shell")).toBeNull();
  });
});
