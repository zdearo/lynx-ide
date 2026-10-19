import type { BranchSync } from "@/bindings";

/**
 * What the button under the file list does. With files staged it commits them. With the index
 * empty it sends what the remote lacks: new commits, or a branch the remote does not have yet.
 */
export type PrimaryAction =
  | { kind: "commit" }
  | { kind: "push"; remote: string; ahead: number; publish: boolean };

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
  if (sync.upstream === null) {
    return { kind: "push", remote: sync.remote, ahead: 0, publish: true };
  }
  if (sync.ahead > 0) {
    return {
      kind: "push",
      remote: sync.remote,
      ahead: sync.ahead,
      publish: false,
    };
  }
  return { kind: "commit" };
}
