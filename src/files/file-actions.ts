import { toast } from "sonner";

import { commands } from "@/bindings";
import { quotePathForShell } from "@/files/shell-quote";
import { errorMessage } from "@/lib/error-message";
import { unwrap } from "@/lib/ipc-result";
import { activeSessionId, useTerminalStore } from "@/terminal/terminal-store";
import { copy } from "@/ui-copy";

export function absolutePath(worktreePath: string, relative: string): string {
  return relative ? `${worktreePath}/${relative}` : worktreePath;
}

export async function copyToClipboard(text: string): Promise<void> {
  try {
    await navigator.clipboard.writeText(text);
    toast.success(copy.files.copied);
  } catch {
    toast.error(copy.files.copyFailed);
  }
}

/** Types the path at the active terminal's prompt, without pressing Enter, so it can be part of a prompt. */
export async function sendPathToTerminal(
  worktreePath: string,
  relative: string,
): Promise<void> {
  const { sessions, activeByWorktree } = useTerminalStore.getState();
  const id = activeSessionId(sessions, activeByWorktree, worktreePath);
  const session = sessions.find((candidate) => candidate.id === id);
  if (!id || !session?.running) {
    toast.info(copy.files.noTerminal);
    return;
  }
  const windows = /Windows/.test(navigator.userAgent);
  // The trailing space lets several paths be sent one after another.
  await commands.writeTerminal(id, `${quotePathForShell(relative, windows)} `);
}

export async function openContainingFolder(
  worktreePath: string,
  relative: string,
): Promise<void> {
  try {
    await unwrap(commands.revealInFileManager(worktreePath, relative));
  } catch (error) {
    toast.error(copy.files.openFolderFailed, {
      description: errorMessage(error),
    });
  }
}
