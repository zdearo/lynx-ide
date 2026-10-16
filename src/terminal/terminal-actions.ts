import { toast } from "sonner";

import { copy } from "@/ui-copy";
import { errorMessage } from "@/lib/error-message";
import { activeSessionId, useTerminalStore } from "@/terminal/terminal-store";
import { useWorkspaceStore } from "@/workspace/workspace-store";

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

export async function closeActiveTerminal(): Promise<void> {
  const path = useWorkspaceStore.getState().selectedWorktreePath;
  const { sessions, activeByWorktree, close } = useTerminalStore.getState();
  const id = activeSessionId(sessions, activeByWorktree, path);
  if (id) await close(id);
}
