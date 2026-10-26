import type { Repo, WorktreeInfo } from "@/bindings";
import { useWorkspaceStore } from "@/workspace/workspace-store";

export interface SelectedWorktree {
  repo: Repo;
  worktree: WorktreeInfo;
}

export function useSelectedWorktree(): SelectedWorktree | null {
  const repos = useWorkspaceStore((state) => state.repos);
  const worktrees = useWorkspaceStore((state) => state.worktrees);
  const selectedPath = useWorkspaceStore((state) => state.selectedWorktreePath);

  if (!selectedPath) return null;
  for (const repo of repos) {
    const worktree = worktrees[repo.id]?.find((w) => w.path === selectedPath);
    if (worktree) return { repo, worktree };
  }
  return null;
}
