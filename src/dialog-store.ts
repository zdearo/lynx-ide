import { create } from "zustand";

import type { Repo, WorktreeInfo } from "@/bindings";

/** Which confirmation or prompt dialog is open, and for what. */
interface DialogState {
  /** Worktree path that a custom command will run in. */
  customCommandFor: string | null;
  removeWorktreeTarget: { repoId: string; worktree: WorktreeInfo } | null;
  removeRepoTarget: Repo | null;

  askCustomCommand: (worktreePath: string | null) => void;
  askRemoveWorktree: (
    target: { repoId: string; worktree: WorktreeInfo } | null,
  ) => void;
  askRemoveRepo: (repo: Repo | null) => void;
}

export const useDialogStore = create<DialogState>((set) => ({
  customCommandFor: null,
  removeWorktreeTarget: null,
  removeRepoTarget: null,

  askCustomCommand: (customCommandFor) => set({ customCommandFor }),
  askRemoveWorktree: (removeWorktreeTarget) => set({ removeWorktreeTarget }),
  askRemoveRepo: (removeRepoTarget) => set({ removeRepoTarget }),
}));
