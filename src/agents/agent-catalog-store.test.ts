import { describe, expect, it } from "vitest";

import type { AgentEntry, HookInstallState } from "@/bindings";
import { onAgents } from "@/agents/agent-catalog-store";

function entry(
  harness: AgentEntry["harness"],
  state: HookInstallState,
  found = true,
): AgentEntry {
  return {
    harness,
    found,
    hooks: { state, configPath: "/x", events: [], detail: null },
  };
}

describe("onAgents", () => {
  it("counts an agent as on only when its hooks are fully installed", () => {
    const on = onAgents([
      entry("claude", "installed"),
      entry("codex", "partial"),
      entry("opencode", "notInstalled"),
    ]);
    expect([...on]).toEqual(["claude"]);
  });

  it("keeps an installed agent on even when its CLI is no longer found", () => {
    const on = onAgents([entry("codex", "installed", false)]);
    expect(on.has("codex")).toBe(true);
  });

  it("is empty while the catalog has not loaded", () => {
    expect(onAgents(null).size).toBe(0);
  });
});
