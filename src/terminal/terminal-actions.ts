import { toast } from "sonner";

import { useConfigStore } from "@/config/config-store";
import { useDialogStore } from "@/dialog-store";
import { copy } from "@/ui-copy";
import { errorMessage } from "@/lib/error-message";
import { launchCommand, type TerminalPreset } from "@/terminal/agent-presets";
import { activeSessionId, useTerminalStore } from "@/terminal/terminal-store";
import { useWorkspaceStore } from "@/workspace/workspace-store";

/** Starts a preset in a worktree, with the launch arguments Settings sets for its agent. */
export function startPreset(
  worktreePath: string,
  preset: TerminalPreset,
): Promise<void> {
  const agents = useConfigStore.getState().snapshot?.config.agents;
  return startTerminal(
    worktreePath,
    launchCommand(
      preset,
      agents?.launchArguments ?? {},
      agents?.launchMode ?? "manual",
    ),
  );
}

export async function startTerminal(
  worktreePath: string,
  command: string | null,
): Promise<void> {
  try {
    await useTerminalStore.getState().create(worktreePath, command);
    // Starting something in a worktree means looking at it.
    useWorkspaceStore.getState().selectWorktree(worktreePath);
  } catch (error) {
    toast.error(copy.terminal.createFailed, {
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
