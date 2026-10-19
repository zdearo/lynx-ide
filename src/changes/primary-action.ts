import type { BranchSync } from "@/bindings";

/**
 * What the button under the file list does. With files staged it commits them. With the index
 * empty it keeps the branch and its remote together: publish or push what the remote lacks, pull
 * what it has, sync when both moved, and force push with lease only when the remote holds old
 * copies of commits that were rewritten here.
 */
export type PrimaryAction =
  | { kind: "commit" }
  | { kind: "push"; remote: string; ahead: number; publish: boolean }
  | { kind: "pull"; remote: string; behind: number }
  | { kind: "sync"; remote: string; ahead: number; behind: number }
  | { kind: "forcePush"; remote: string; replaced: number };

export function primaryAction(
  stagedCount: number,
  sync: BranchSync | undefined,
): PrimaryAction {
  if (
    stagedCount > 0 ||
    !sync ||
    sync.branch === null ||
    sync.remote === null
  ) {
    return { kind: "commit" };
  }
  const { remote, ahead, behind } = sync;
  if (sync.upstream === null) {
    return { kind: "push", remote, ahead: 0, publish: true };
  }
  if (sync.rewritten) return { kind: "forcePush", remote, replaced: behind };
  if (ahead > 0 && behind > 0) return { kind: "sync", remote, ahead, behind };
  if (behind > 0) return { kind: "pull", remote, behind };
  if (ahead > 0) return { kind: "push", remote, ahead, publish: false };
  return { kind: "commit" };
}
