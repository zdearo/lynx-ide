import { describe, expect, it } from "vitest";

import type { AppConfig } from "@/bindings";
import { DEFAULT_TERMINAL_CONFIG } from "@/config/config-store";
import {
  launchCommand,
  TERMINAL_PRESETS,
  visiblePresets,
  withLaunchArgument,
  withLaunchMode,
  yoloFlag,
} from "@/terminal/agent-presets";

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
    expect(every.size).toBe(12);
    expect(visiblePresets(every).map((preset) => preset.id)).toHaveLength(13);
  });
});

function preset(id: string) {
  const found = TERMINAL_PRESETS.find((candidate) => candidate.id === id);
  if (!found) throw new Error(`no preset ${id}`);
  return found;
}

const config: AppConfig = {
  terminal: DEFAULT_TERMINAL_CONFIG,
  worktrees: { directory: null },
  agents: { launchMode: "manual", launchArguments: {} },
  notifications: { onDone: true, onNeedsYou: true, onUpdate: true },
  updates: { checkAutomatically: true },
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
};

describe("launchCommand", () => {
  it("types the agent's command followed by the arguments turned on for it", () => {
    expect(
      launchCommand(preset("claude"), { claude: "--model opus" }, "manual"),
    ).toBe("claude --model opus");
  });

  it("is the plain command when the agent's arguments are off", () => {
    expect(launchCommand(preset("claude"), { codex: "--x" }, "manual")).toBe(
      "claude",
    );
    expect(launchCommand(preset("claude"), {}, "manual")).toBe("claude");
  });

  it("adds the agent's YOLO flag in YOLO mode, before its arguments", () => {
    expect(
      launchCommand(preset("claude"), { claude: "--model opus" }, "yolo"),
    ).toBe("claude --dangerously-skip-permissions --model opus");
  });

  it("adds only the YOLO flag when the arguments are on but empty", () => {
    expect(launchCommand(preset("codex"), { codex: "" }, "yolo")).toBe(
      "codex --dangerously-bypass-approvals-and-sandbox",
    );
  });

  it("starts an agent without a known YOLO flag as it is in YOLO mode", () => {
    expect(launchCommand(preset("pi"), {}, "yolo")).toBe("pi");
  });

  it("keeps Kiro's flags after its chat subcommand", () => {
    expect(launchCommand(preset("kiro"), {}, "yolo")).toBe(
      "kiro-cli chat --trust-all-tools",
    );
  });

  it("gives a plain shell no command, whatever is set", () => {
    expect(
      launchCommand(preset("shell"), { claude: "--model opus" }, "yolo"),
    ).toBeNull();
  });
});

describe("yoloFlag", () => {
  it("names the flag of each agent that has one", () => {
    expect(yoloFlag("claude")).toBe("--dangerously-skip-permissions");
    expect(yoloFlag("omp")).toBe("--auto-approve");
    expect(yoloFlag("pi")).toBeNull();
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
