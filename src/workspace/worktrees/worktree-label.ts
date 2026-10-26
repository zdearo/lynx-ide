import type { WorktreeInfo } from "@/bindings";

/** Label for a worktree row: its branch, or the short commit when HEAD is detached. */
export function worktreeLabel(worktree: WorktreeInfo): {
  text: string;
  detached: boolean;
} {
  if (worktree.branch) return { text: worktree.branch, detached: false };
  return { text: worktree.head?.slice(0, 7) ?? worktree.name, detached: true };
}
