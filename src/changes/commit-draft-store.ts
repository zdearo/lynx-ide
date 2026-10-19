import { create } from "zustand";

export interface CommitDraft {
  summary: string;
  description: string;
}

const EMPTY: CommitDraft = { summary: "", description: "" };

interface CommitDraftState {
  /** What the user typed for the next commit, per worktree. Kept while the app runs. */
  drafts: Record<string, CommitDraft>;
  update: (worktreePath: string, change: Partial<CommitDraft>) => void;
  clear: (worktreePath: string) => void;
}

export const useCommitDraftStore = create<CommitDraftState>((set) => ({
  drafts: {},
  update(worktreePath, change) {
    set((state) => ({
      drafts: {
        ...state.drafts,
        [worktreePath]: {
          ...(state.drafts[worktreePath] ?? EMPTY),
          ...change,
        },
      },
    }));
  },
  clear(worktreePath) {
    set((state) => ({
      drafts: { ...state.drafts, [worktreePath]: EMPTY },
    }));
  },
}));

/** A worktree's draft, or an empty one. Stable object, so selectors do not loop. */
export function draftOf(
  drafts: Record<string, CommitDraft>,
  worktreePath: string,
): CommitDraft {
  return drafts[worktreePath] ?? EMPTY;
}
