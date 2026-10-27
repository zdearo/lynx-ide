import { beforeEach, describe, expect, it, vi } from "vitest";

import { commands, type ConfigSnapshot } from "@/bindings";
import {
  DEFAULT_BOTS_CONFIG,
  DEFAULT_TERMINAL_CONFIG,
  friendlyConfigError,
  useConfigStore,
} from "@/config/config-store";
import { failure, ok } from "@/test-support";

vi.mock("@/bindings", () => ({
  commands: { getConfig: vi.fn(), updateConfig: vi.fn() },
}));

const api = vi.mocked(commands);
const initialState = useConfigStore.getState();

function snapshot(fontSize: number): ConfigSnapshot {
  return {
    config: {
      terminal: { ...DEFAULT_TERMINAL_CONFIG, fontSize },
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
    },
    issues: [],
    path: "/home/dev/.lynx/config.json",
    fileExists: true,
    fileBroken: false,
  };
}

beforeEach(() => {
  vi.resetAllMocks();
  useConfigStore.setState(initialState, true);
});

describe("config store", () => {
  it("loads the snapshot", async () => {
    api.getConfig.mockResolvedValue(ok(snapshot(13)));
    await useConfigStore.getState().load();
    expect(useConfigStore.getState().snapshot?.config.terminal.fontSize).toBe(
      13,
    );
  });

  it("keeps the saved snapshot the backend returns", async () => {
    const next = snapshot(16);
    api.updateConfig.mockResolvedValue(ok(next));

    await useConfigStore.getState().save(next.config);

    expect(api.updateConfig).toHaveBeenCalledWith(next.config);
    expect(useConfigStore.getState().snapshot).toEqual(next);
  });

  it("rejects with the reason and keeps the previous snapshot when the backend refuses", async () => {
    useConfigStore.setState({ snapshot: snapshot(13) });
    api.updateConfig.mockResolvedValue(
      failure("invalidConfig", "terminal.fontSize: must be between 8 and 32"),
    );

    await expect(
      useConfigStore.getState().save(snapshot(99).config),
    ).rejects.toMatchObject({ kind: "invalidConfig" });
    expect(useConfigStore.getState().snapshot?.config.terminal.fontSize).toBe(
      13,
    );
  });

  it("applies a snapshot pushed after an outside edit", () => {
    useConfigStore.getState().apply(snapshot(20));
    expect(useConfigStore.getState().snapshot?.config.terminal.fontSize).toBe(
      20,
    );
  });
});

describe("bots defaults", () => {
  it("keeps the ChatGPT plan off until the user turns it on", () => {
    expect(DEFAULT_BOTS_CONFIG).toEqual({
      chatgpt: { enabled: false },
      decider: { enabled: false },
      memory: { capture: true },
    });
  });
});

describe("friendlyConfigError", () => {
  it("drops the key prefix", () => {
    expect(
      friendlyConfigError("terminal.fontSize: must be between 8 and 32"),
    ).toBe("must be between 8 and 32");
  });

  it("leaves messages without a prefix alone", () => {
    expect(friendlyConfigError("something broke")).toBe("something broke");
  });
});
