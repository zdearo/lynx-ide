import { create } from "zustand";

import { commands, type BranchSync } from "@/bindings";
import { unwrap } from "@/lib/ipc-result";

interface BranchSyncState {
  byWorktree: Record<string, BranchSync>;
  /** Reads where the branch stands against its remote. A failed read keeps the last answer. */
  refresh: (worktreePath: string) => Promise<void>;
}

export const useBranchSyncStore = create<BranchSyncState>((set) => ({
  byWorktree: {},

  async refresh(worktreePath) {
    try {
      const sync = await unwrap(commands.getBranchSync(worktreePath));
      set((state) => ({
        byWorktree: { ...state.byWorktree, [worktreePath]: sync },
      }));
    } catch {
      // The commit box works without it; the next read tries again.
    }
  },
}));
