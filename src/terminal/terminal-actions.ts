import { toast } from "sonner";

import type { HarnessId } from "@/bindings";
import { commands } from "@/bindings";
import { useConfigStore } from "@/config/config-store";
import { useDialogStore } from "@/dialog-store";
import { copy } from "@/ui-copy";
import { errorMessage } from "@/lib/error-message";
import { TERMINAL_PRESETS, type TerminalPreset } from "@/agents/agent-presets";
import { useTeamStore } from "@/team/team-store";
import { activeSessionId, useTerminalStore } from "@/terminal/terminal-store";
import { useWorkspaceStore } from "@/workspace/workspace-store";

/** The line that starts a preset's agent, as the backend builds it from Settings. Null for a shell. */
export async function launchLine(
  preset: TerminalPreset,
): Promise<string | null> {
  if (preset.harness === null) return null;
  return commands.agentLaunchLine(preset.harness);
}

/** The line that starts a harness's agent as a new conversation. Null when the harness has no preset. */
export async function agentLine(harness: HarnessId): Promise<string | null> {
  const preset = TERMINAL_PRESETS.find(
    (candidate) => candidate.harness === harness,
  );
  return preset ? launchLine(preset) : null;
}

/** Starts a preset in a worktree, with the launch arguments Settings sets for its agent. */
export async function startPreset(
  worktreePath: string,
  preset: TerminalPreset,
): Promise<void> {
  await startTerminal(worktreePath, await launchLine(preset));
}

export async function startTerminal(
  worktreePath: string,
  command: string | null,
): Promise<boolean> {
  try {
    await useTerminalStore.getState().create(worktreePath, command);
    // Only the person starts a terminal here, so only this brings it in front of the team. A session the backend or
    // the team starts never comes through this function.
    useTeamStore.getState().setMode(worktreePath, "terminal");
    // Starting something in a worktree means looking at it.
    useWorkspaceStore.getState().selectWorktree(worktreePath);
    return true;
  } catch (error) {
    toast.error(copy.terminal.createFailed, {
      description: errorMessage(error),
    });
    return false;
  }
}

/** Starts again a session that was open when the app quit, under the same id. */
export async function restoreSession(id: string): Promise<void> {
  try {
    await useTerminalStore.getState().restore(id);
  } catch (error) {
    toast.error(copy.terminal.restore.failed, {
      description: errorMessage(error),
    });
  }
}

export async function sleepTerminal(id: string): Promise<void> {
  try {
    await useTerminalStore.getState().hibernate(id);
  } catch (error) {
    toast.error(copy.terminal.sleepFailed, {
      description: errorMessage(error),
    });
  }
}

export async function startShellInSelectedWorktree(): Promise<void> {
  const path = useWorkspaceStore.getState().selectedWorktreePath;
  if (path) await startTerminal(path, null);
}

/**
 * Every user-initiated close goes through here, so the confirm setting applies
 * to the tab button, the shortcut and an agent's close button alike.
 */
export function requestCloseTerminal(id: string): void {
  const session = useTerminalStore
    .getState()
    .sessions.find((candidate) => candidate.id === id);
  const askFirst =
    useConfigStore.getState().snapshot?.config.terminal.confirmClose ?? false;
  if (session?.running && askFirst) {
    useDialogStore.getState().askCloseTerminal(id);
  } else {
    void useTerminalStore.getState().close(id);
  }
}

export async function closeActiveTerminal(): Promise<void> {
  const path = useWorkspaceStore.getState().selectedWorktreePath;
  const { sessions, activeByWorktree } = useTerminalStore.getState();
  const id = activeSessionId(sessions, activeByWorktree, path);
  if (id) requestCloseTerminal(id);
}

/** A program ended: keep its last output, or close the tab when the setting says so. */
export function handleTerminalExit(id: string, exitCode: number | null): void {
  const store = useTerminalStore.getState();
  store.markExited(id, exitCode);
  const closeOnExit =
    useConfigStore.getState().snapshot?.config.terminal.closeOnExit ?? false;
  if (closeOnExit) void store.close(id);
}
