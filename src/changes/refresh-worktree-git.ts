import { useBranchSyncStore } from "@/changes/branch-sync-store";
import { useChangesStore } from "@/changes/changes-store";
import { useCommitsStore } from "@/changes/commits-store";

/** Everything the Changes tab shows about a worktree's Git state, read again together. */
export function refreshWorktreeGit(worktreePath: string): Promise<unknown> {
  return Promise.all([
    useChangesStore.getState().refresh(worktreePath),
    useBranchSyncStore.getState().refresh(worktreePath),
    useCommitsStore.getState().refresh(worktreePath),
  ]);
}
