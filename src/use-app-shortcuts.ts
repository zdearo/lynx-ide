import { useEffect } from "react";

import { matchShortcut } from "@/platform-shortcuts";
import {
  closeActiveTerminal,
  startShellInSelectedWorktree,
} from "@/terminal/terminal-actions";
import { useWorkspaceStore } from "@/workspace/workspace-store";

function openNewWorktreeForSelection() {
  const { repos, worktrees, selectedWorktreePath, openNewWorktree } =
    useWorkspaceStore.getState();
  const owner = repos.find((repo) =>
    worktrees[repo.id]?.some((w) => w.path === selectedWorktreePath),
  );
  const target = owner ?? repos[0];
  if (target) openNewWorktree(target.id);
}

/** Capture phase, so the terminal never sees a key the app has claimed. */
export function useAppShortcuts(toggleSidebar: () => void) {
  useEffect(() => {
    const onKeyDown = (event: KeyboardEvent) => {
      const action = matchShortcut(event);
      if (!action) return;
      event.preventDefault();
      event.stopPropagation();

      switch (action) {
        case "newTerminal":
          void startShellInSelectedWorktree();
          break;
        case "closeTerminal":
          void closeActiveTerminal();
          break;
        case "newWorktree":
          openNewWorktreeForSelection();
          break;
        case "toggleSidebar":
          toggleSidebar();
          break;
      }
    };
    window.addEventListener("keydown", onKeyDown, { capture: true });
    return () =>
      window.removeEventListener("keydown", onKeyDown, { capture: true });
  }, [toggleSidebar]);
}
