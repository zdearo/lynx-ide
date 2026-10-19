import { create } from "zustand";

import { commands, type IpcErrorKind, type PullRequest } from "@/bindings";
import { errorMessage } from "@/lib/error-message";
import { IpcRequestError, unwrap } from "@/lib/ipc-result";

export type PullRequestView =
  | { status: "loading" }
  | { status: "ready"; pullRequest: PullRequest | null }
  | { status: "error"; kind: IpcErrorKind; message: string };

interface PullRequestStoreState {
  byWorktree: Record<string, PullRequestView>;
  /** Asks GitHub again for the pull request of the worktree's branch. */
  refresh: (worktreePath: string) => Promise<void>;
}

export const usePullRequestStore = create<PullRequestStoreState>((set) => ({
  byWorktree: {},

  async refresh(worktreePath) {
    let view: PullRequestView;
    try {
      const pullRequest = await unwrap(commands.getPullRequest(worktreePath));
      view = { status: "ready", pullRequest };
    } catch (error) {
      view =
        error instanceof IpcRequestError
          ? { status: "error", kind: error.kind, message: error.message }
          : { status: "error", kind: "internal", message: errorMessage(error) };
    }
    set((state) => ({
      byWorktree: { ...state.byWorktree, [worktreePath]: view },
    }));
  },
}));
