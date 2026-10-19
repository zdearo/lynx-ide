import { create } from "zustand";

import { commands, type Discussion } from "@/bindings";
import { errorMessage } from "@/lib/error-message";
import { unwrap } from "@/lib/ipc-result";

export type DiscussionView =
  | { status: "loading" }
  | { status: "ready"; discussion: Discussion; readAt: number }
  | { status: "error"; message: string };

interface DiscussionState {
  byWorktree: Record<string, DiscussionView>;
  refresh: (worktreePath: string, number: number) => Promise<void>;
}

export const usePullRequestDiscussionStore = create<DiscussionState>((set) => ({
  byWorktree: {},

  async refresh(worktreePath, number) {
    let view: DiscussionView;
    try {
      const discussion = await unwrap(
        commands.getPullRequestDiscussion(worktreePath, number),
      );
      view = { status: "ready", discussion, readAt: Date.now() };
    } catch (error) {
      view = { status: "error", message: errorMessage(error) };
    }
    set((state) => ({
      byWorktree: { ...state.byWorktree, [worktreePath]: view },
    }));
  },
}));
