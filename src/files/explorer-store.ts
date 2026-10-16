import { create } from "zustand";

import { commands } from "@/bindings";
import {
  childPath,
  type DirectoryState,
  type TreeState,
} from "@/files/file-tree-rows";
import { errorMessage } from "@/lib/error-message";
import { IpcRequestError, unwrap } from "@/lib/ipc-result";

const EMPTY_TREE: TreeState = { directories: {}, expanded: {}, selected: null };

interface ExplorerState {
  trees: Record<string, TreeState>;

  /** Loads one folder. Entries already on screen stay put while it reloads. */
  load: (worktreePath: string, directory: string) => Promise<void>;
  /** Opens or closes a folder, loading it the first time it opens. */
  toggle: (worktreePath: string, directory: string) => Promise<void>;
  /** Reloads the root and every open folder, e.g. after files changed on disk. */
  refresh: (worktreePath: string) => Promise<void>;
  collapseAll: (worktreePath: string) => void;
  select: (worktreePath: string, path: string | null) => void;
}

export const useExplorerStore = create<ExplorerState>((set, get) => {
  const update = (
    worktreePath: string,
    change: (tree: TreeState) => TreeState,
  ) =>
    set((state) => ({
      trees: {
        ...state.trees,
        [worktreePath]: change(state.trees[worktreePath] ?? EMPTY_TREE),
      },
    }));

  const setDirectory = (
    worktreePath: string,
    directory: string,
    next: DirectoryState,
  ) =>
    update(worktreePath, (tree) => ({
      ...tree,
      directories: { ...tree.directories, [directory]: next },
    }));

  return {
    trees: {},

    async load(worktreePath, directory) {
      const previous = get().trees[worktreePath]?.directories[directory];
      setDirectory(worktreePath, directory, {
        status: "loading",
        entries: previous?.entries ?? [],
        truncated: previous?.truncated ?? false,
        error: null,
      });
      try {
        const listing = await unwrap(
          commands.listDirectory(worktreePath, directory),
        );
        setDirectory(worktreePath, directory, {
          status: "ready",
          entries: listing.entries,
          truncated: listing.truncated,
          error: null,
        });
      } catch (error) {
        if (error instanceof IpcRequestError && error.kind === "pathNotFound") {
          // The folder was deleted from disk: close it instead of showing an error.
          update(worktreePath, (tree) => {
            const expanded = { ...tree.expanded };
            delete expanded[directory];
            const directories = { ...tree.directories };
            delete directories[directory];
            return { ...tree, expanded, directories };
          });
          return;
        }
        setDirectory(worktreePath, directory, {
          status: "error",
          entries: previous?.entries ?? [],
          truncated: false,
          error: errorMessage(error),
        });
      }
    },

    async toggle(worktreePath, directory) {
      const tree = get().trees[worktreePath] ?? EMPTY_TREE;
      if (tree.expanded[directory]) {
        update(worktreePath, (current) => {
          const expanded = { ...current.expanded };
          delete expanded[directory];
          return { ...current, expanded };
        });
        return;
      }
      update(worktreePath, (current) => ({
        ...current,
        expanded: { ...current.expanded, [directory]: true },
      }));
      if (tree.directories[directory]?.status !== "ready") {
        await get().load(worktreePath, directory);
      }
    },

    async refresh(worktreePath) {
      const tree = get().trees[worktreePath] ?? EMPTY_TREE;
      const open = Object.keys(tree.expanded);
      await Promise.all(
        ["", ...open].map((directory) => get().load(worktreePath, directory)),
      );
    },

    collapseAll(worktreePath) {
      update(worktreePath, (tree) => ({ ...tree, expanded: {} }));
    },

    select(worktreePath, path) {
      update(worktreePath, (tree) => ({ ...tree, selected: path }));
    },
  };
});

export { childPath };
