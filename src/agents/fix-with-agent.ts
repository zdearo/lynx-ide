import { toast } from "sonner";

import {
  agentCommand,
  pickFixAgent,
  promptInstruction,
} from "@/agents/fix-prompts";
import { onAgents, useAgentCatalogStore } from "@/agents/agent-catalog-store";
import { commands } from "@/bindings";
import { useConfigStore } from "@/config/config-store";
import { errorMessage } from "@/lib/error-message";
import { unwrap } from "@/lib/ipc-result";
import { visiblePresets } from "@/terminal/agent-presets";
import { startTerminal } from "@/terminal/terminal-actions";
import { copy } from "@/ui-copy";

/** Whether an agent that takes a first prompt is on, so the fix buttons have someone to hand it to. */
export function useHasFixAgent(): boolean {
  const entries = useAgentCatalogStore((state) => state.entries);
  const configured = useConfigStore(
    (state) => state.snapshot?.config.git.commitMessageAgent ?? "auto",
  );
  return pickFixAgent(visiblePresets(onAgents(entries)), configured) !== null;
}

function presetsOn() {
  const entries = useAgentCatalogStore.getState().entries;
  return visiblePresets(onAgents(entries));
}

function configuredAgent() {
  return (
    useConfigStore.getState().snapshot?.config.git.commitMessageAgent ?? "auto"
  );
}

/**
 * Saves the prompt to a file and starts an agent in the worktree pointing at it, so a long log
 * never has to travel on the command line. Returns whether an agent was started.
 */
export async function fixWithAgent(
  worktreePath: string,
  prompt: string,
): Promise<boolean> {
  const preset = pickFixAgent(presetsOn(), configuredAgent());
  if (!preset) {
    toast.error(copy.fixWithAi.noAgent);
    return false;
  }
  try {
    const path = await unwrap(commands.writeAgentPrompt(prompt));
    const agents = useConfigStore.getState().snapshot?.config.agents;
    const command = agentCommand(
      preset,
      agents?.launchArguments ?? {},
      agents?.launchMode ?? "manual",
      promptInstruction(path),
      /Windows/.test(navigator.userAgent),
    );
    await startTerminal(worktreePath, command);
    toast.success(copy.fixWithAi.started(preset.label));
    return true;
  } catch (error) {
    toast.error(copy.fixWithAi.failed, { description: errorMessage(error) });
    return false;
  }
}
