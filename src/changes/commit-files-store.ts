import { create } from "zustand";

import { commands, type CommitFile } from "@/bindings";
import { unwrap } from "@/lib/ipc-result";

export type CommitFilesView =
  | { status: "loading" }
  | { status: "ready"; files: CommitFile[] }
  | { status: "error" };

interface CommitFilesState {
  /** Keyed by worktree and hash: a commit never changes, so a read is kept. */
  byCommit: Record<string, CommitFilesView>;
  load: (worktreePath: string, hash: string) => Promise<void>;
}

export const commitFilesKey = (worktreePath: string, hash: string) =>
  `${worktreePath}::${hash}`;

export const useCommitFilesStore = create<CommitFilesState>((set, get) => ({
  byCommit: {},

  async load(worktreePath, hash) {
    const key = commitFilesKey(worktreePath, hash);
    if (get().byCommit[key]?.status === "ready") return;
    set((state) => ({
      byCommit: { ...state.byCommit, [key]: { status: "loading" } },
    }));
    try {
      const files = await unwrap(commands.listCommitFiles(worktreePath, hash));
      set((state) => ({
        byCommit: { ...state.byCommit, [key]: { status: "ready", files } },
      }));
    } catch {
      set((state) => ({
        byCommit: { ...state.byCommit, [key]: { status: "error" } },
      }));
    }
  },
}));
