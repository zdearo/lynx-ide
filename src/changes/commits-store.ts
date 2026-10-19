import { create } from "zustand";

import { commands, type CommitEntry } from "@/bindings";
import { unwrap } from "@/lib/ipc-result";

/** How many commits the list asks for; older ones are in the terminal's `git log`. */
const LIMIT = 50;

/** The commits, and when they were read, so "5 min ago" is stable between reads. */
export interface CommitsRead {
  commits: CommitEntry[];
  readAt: number;
}

interface CommitsState {
  byWorktree: Record<string, CommitsRead>;
  /** Reads the branch's latest commits. A failed read keeps the last list. */
  refresh: (worktreePath: string) => Promise<void>;
}

export const useCommitsStore = create<CommitsState>((set) => ({
  byWorktree: {},

  async refresh(worktreePath) {
    try {
      const commits = await unwrap(commands.listCommits(worktreePath, LIMIT));
      set((state) => ({
        byWorktree: {
          ...state.byWorktree,
          [worktreePath]: { commits, readAt: Date.now() },
        },
      }));
    } catch {
      // The next read tries again.
    }
  },
}));
