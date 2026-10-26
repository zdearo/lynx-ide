import { useMemo } from "react";
import { create } from "zustand";

import { commands, type AgentEntry, type HarnessId } from "@/bindings";
import { unwrap } from "@/lib/ipc-result";
import { visiblePresets, type TerminalPreset } from "@/agents/agent-presets";

interface AgentCatalogState {
  /** Null until the first load finishes. */
  entries: AgentEntry[] | null;
  load: () => Promise<void>;
}

/** The supported agents and what this computer shows about each. */
export const useAgentCatalogStore = create<AgentCatalogState>((set) => ({
  entries: null,
  async load() {
    set({ entries: await unwrap(commands.agentCatalog()) });
  },
}));

/** Agents that are on: their hooks are in place, so Lynx follows them and offers them. */
export function onAgents(
  entries: readonly AgentEntry[] | null,
): Set<HarnessId> {
  return new Set(
    (entries ?? [])
      .filter((entry) => entry.hooks.state === "installed")
      .map((entry) => entry.harness),
  );
}

/** The new-terminal presets for the agents that are on right now. */
export function useVisiblePresets(): readonly TerminalPreset[] {
  const entries = useAgentCatalogStore((state) => state.entries);
  return useMemo(() => visiblePresets(onAgents(entries)), [entries]);
}
