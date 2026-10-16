import { describe, expect, it } from "vitest";

import { TERMINAL_PRESETS, visiblePresets } from "@/terminal/agent-presets";

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
