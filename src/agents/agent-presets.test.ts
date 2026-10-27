import { describe, expect, it } from "vitest";

import type { AppConfig } from "@/bindings";
import { DEFAULT_TERMINAL_CONFIG } from "@/config/config-store";
import {
  TERMINAL_PRESETS,
  visiblePresets,
  withLaunchArgument,
  withLaunchMode,
  yoloFlag,
} from "@/agents/agent-presets";

describe("visiblePresets", () => {
  it("offers only shells while no agent is on", () => {
    expect(visiblePresets(new Set()).map((preset) => preset.id)).toEqual([
      "shell",
    ]);
  });

  it("adds each agent that is on, and keeps the shell", () => {
    const ids = visiblePresets(new Set(["codex"])).map((preset) => preset.id);
    expect(ids).toEqual(["shell", "codex"]);
  });

  it("offers every agent the settings list, so each one can be started", () => {
    const every = new Set(
      TERMINAL_PRESETS.flatMap((preset) =>
        preset.harness ? [preset.harness] : [],
      ),
    );
    expect(every.size).toBe(4);
    expect(visiblePresets(every).map((preset) => preset.id)).toHaveLength(5);
  });
});

const config: AppConfig = {
  terminal: DEFAULT_TERMINAL_CONFIG,
  worktrees: { directory: null },
  agents: { launchMode: "manual", launchArguments: {} },
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
};

describe("yoloFlag", () => {
  it("names the flag of each agent that has one", () => {
    expect(yoloFlag("claude")).toBe("--dangerously-skip-permissions");
    expect(yoloFlag("omp")).toBe("--auto-approve");
  });
});

describe("withLaunchArgument", () => {
  it("turns one agent's arguments on with a text and keeps the others", () => {
    const first = withLaunchArgument(config, "claude", "--model opus");
    const second = withLaunchArgument(first, "codex", "--full-auto");
    expect(second.agents.launchArguments).toEqual({
      claude: "--model opus",
      codex: "--full-auto",
    });
  });

  it("turns an agent's arguments on with an empty text, which is still on", () => {
    const on = withLaunchArgument(config, "claude", "");
    expect(on.agents.launchArguments).toEqual({ claude: "" });
  });

  it("turns an agent's arguments off when the text is null", () => {
    const set = withLaunchArgument(config, "claude", "--model opus");
    expect(
      withLaunchArgument(set, "claude", null).agents.launchArguments,
    ).toEqual({});
  });
});

describe("withLaunchMode", () => {
  it("sets the mode and keeps the arguments", () => {
    const withArgs = withLaunchArgument(config, "claude", "");
    const yolo = withLaunchMode(withArgs, "yolo");
    expect(yolo.agents).toEqual({
      launchMode: "yolo",
      launchArguments: { claude: "" },
    });
  });
});
