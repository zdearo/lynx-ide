import { create } from "zustand";

import { commands, type ResumableSession } from "@/bindings";
import { unwrap } from "@/lib/ipc-result";

interface ResumableState {
  /** Ended agent sessions per worktree that can be resumed, newest first. */
  byWorktree: Record<string, ResumableSession[]>;
  refresh: (worktreePath: string) => Promise<void>;
  /** The session is being resumed, so it is no longer offered. */
  forget: (worktreePath: string, id: string) => void;
  /** Takes the conversation off the list for good. The agent keeps the conversation. */
  remove: (worktreePath: string, id: string) => Promise<void>;
}

export const useResumableStore = create<ResumableState>((set) => ({
  byWorktree: {},

  async refresh(worktreePath) {
    // A list that cannot be read shows nothing; it is read again on the next change.
    try {
      const result = await commands.listResumableSessions(worktreePath);
      if (result.status !== "ok") return;
      set((state) => ({
        byWorktree: { ...state.byWorktree, [worktreePath]: result.data },
      }));
    } catch {
      // Outside the app there is no backend to ask.
    }
  },

  forget(worktreePath, id) {
    set((state) => ({
      byWorktree: {
        ...state.byWorktree,
        [worktreePath]: (state.byWorktree[worktreePath] ?? []).filter(
          (session) => session.id !== id,
        ),
      },
    }));
  },

  async remove(worktreePath, id) {
    await unwrap(commands.forgetResumableSession(id));
    useResumableStore.getState().forget(worktreePath, id);
  },
}));
