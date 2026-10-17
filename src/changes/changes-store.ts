import { create } from "zustand";

import { commands, type ChangedFile } from "@/bindings";
import { errorMessage } from "@/lib/error-message";
import { unwrap } from "@/lib/ipc-result";

export interface WorktreeChanges {
  files: ChangedFile[];
  status: "loading" | "ready" | "error";
  error: string | null;
  /** Counts every successful read, so a diff on screen knows to look again. */
  tick: number;
}

const NONE: WorktreeChanges = {
  files: [],
  status: "loading",
  error: null,
  tick: 0,
};

interface ChangesStoreState {
  byWorktree: Record<string, WorktreeChanges>;
  /** Reads the changed files again. The previous list stays on screen meanwhile. */
  refresh: (worktreePath: string) => Promise<void>;
}

function sameFiles(a: ChangedFile[], b: ChangedFile[]): boolean {
  return JSON.stringify(a) === JSON.stringify(b);
}

export const useChangesStore = create<ChangesStoreState>((set, get) => ({
  byWorktree: {},

  async refresh(worktreePath) {
    const before = get().byWorktree[worktreePath] ?? NONE;
    try {
      const files = await unwrap(commands.listChanges(worktreePath));
      set((state) => {
        const current = state.byWorktree[worktreePath] ?? NONE;
        return {
          byWorktree: {
            ...state.byWorktree,
            [worktreePath]: {
              // Same list, same array: rows do not redraw for nothing.
              files: sameFiles(current.files, files) ? current.files : files,
              status: "ready",
              error: null,
              tick: current.tick + 1,
            },
          },
        };
      });
    } catch (error) {
      set((state) => ({
        byWorktree: {
          ...state.byWorktree,
          [worktreePath]: {
            ...before,
            status: before.files.length ? before.status : "error",
            error: errorMessage(error),
          },
        },
      }));
    }
  },
}));

/** The change entry of one path, or undefined when the file is not modified. */
export function changeOf(
  changes: WorktreeChanges | undefined,
  path: string,
): ChangedFile | undefined {
  return changes?.files.find((file) => file.path === path);
}
