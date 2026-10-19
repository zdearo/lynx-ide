import type { WorktreeChanged } from "@/bindings";
import { refreshWorktreeGit } from "@/changes/refresh-worktree-git";
import { useChangesStore } from "@/changes/changes-store";
import { useExplorerStore } from "@/files/explorer-store";
import { directoriesToReload } from "@/watching/worktree-change";

/** Reads again what a burst of file changes made out of date, for the panels that show that worktree. */
export function applyWorktreeChange(change: WorktreeChanged): void {
  const { worktreePath } = change;

  const explorer = useExplorerStore.getState();
  for (const directory of directoriesToReload(
    explorer.trees[worktreePath],
    change,
  )) {
    void explorer.load(worktreePath, directory);
  }

  if (change.git) {
    // The index or HEAD moved: staged files, the branch and the commits may all differ.
    void refreshWorktreeGit(worktreePath);
  } else if (useChangesStore.getState().byWorktree[worktreePath]) {
    void useChangesStore.getState().refresh(worktreePath);
  }
}
