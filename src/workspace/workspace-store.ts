import { create } from "zustand";

import {
  commands,
  type RemoveWorktreeResult,
  type Repo,
  type WorktreeInfo,
} from "@/bindings";
import { IpcRequestError, unwrap } from "@/lib/ipc-result";

const SELECTED_WORKTREE_KEY = "agent-ade.selectedWorktree";

let nextPendingId = 0;

export interface PendingWorktree {
  id: string;
  repoId: string;
  name: string;
}

/** The "New worktree" dialog is open exactly when a draft exists. */
export interface WorktreeDraft {
  repoId: string;
  name: string;
  baseRef: string | null;
  error: string | null;
}

interface WorkspaceState {
  loaded: boolean;
  repos: Repo[];
  worktrees: Record<string, WorktreeInfo[]>;
  collapsedRepoIds: Record<string, boolean>;
  selectedWorktreePath: string | null;
  pending: PendingWorktree[];
  draft: WorktreeDraft | null;

  load: () => Promise<void>;
  refreshAll: () => Promise<void>;
  refreshWorktrees: (repoId: string) => Promise<void>;
  addRepo: (path: string) => Promise<Repo>;
  removeRepo: (repoId: string) => Promise<void>;
  selectWorktree: (path: string) => void;
  toggleRepo: (repoId: string) => void;
  openNewWorktree: (repoId: string) => void;
  closeNewWorktree: () => void;
  createWorktree: (draft: WorktreeDraft) => Promise<void>;
  removeWorktree: (
    repoId: string,
    path: string,
    force: boolean,
  ) => Promise<RemoveWorktreeResult>;
}

export const useWorkspaceStore = create<WorkspaceState>((set, get) => {
  const select = (path: string | null) => {
    set({ selectedWorktreePath: path });
    rememberSelection(path);
  };

  /** Keeps the selection pointing at a worktree that still exists. */
  const reconcileSelection = () => {
    const { worktrees, repos, selectedWorktreePath } = get();
    const all = Object.values(worktrees).flat();
    if (
      selectedWorktreePath &&
      all.some((w) => w.path === selectedWorktreePath)
    )
      return;
    const firstMain = repos
      .map((repo) => worktrees[repo.id]?.[0])
      .find((worktree) => worktree !== undefined);
    select(firstMain?.path ?? null);
  };

  return {
    loaded: false,
    repos: [],
    worktrees: {},
    collapsedRepoIds: {},
    selectedWorktreePath: readRememberedSelection(),
    pending: [],
    draft: null,

    async load() {
      const repos = await unwrap(commands.listRepos());
      set({ repos });
      await get().refreshAll();
      set({ loaded: true });
    },

    async refreshAll() {
      await Promise.all(
        get().repos.map((repo) => get().refreshWorktrees(repo.id)),
      );
      reconcileSelection();
    },

    async refreshWorktrees(repoId) {
      try {
        const list = await unwrap(commands.listWorktrees(repoId));
        set((state) => ({ worktrees: { ...state.worktrees, [repoId]: list } }));
      } catch (error) {
        // A repo whose folder vanished keeps its last known worktrees instead of blanking the sidebar.
        if (!(error instanceof IpcRequestError)) throw error;
      }
    },

    async addRepo(path) {
      const repo = await unwrap(commands.addRepo(path));
      set((state) => ({
        repos: state.repos.some((r) => r.id === repo.id)
          ? state.repos
          : [...state.repos, repo],
        collapsedRepoIds: { ...state.collapsedRepoIds, [repo.id]: false },
      }));
      await get().refreshWorktrees(repo.id);
      const main = get().worktrees[repo.id]?.[0];
      if (main) select(main.path);
      return repo;
    },

    async removeRepo(repoId) {
      await unwrap(commands.removeRepo(repoId));
      set((state) => {
        const worktrees = { ...state.worktrees };
        delete worktrees[repoId];
        return {
          repos: state.repos.filter((repo) => repo.id !== repoId),
          worktrees,
        };
      });
      reconcileSelection();
    },

    selectWorktree: select,

    toggleRepo(repoId) {
      set((state) => ({
        collapsedRepoIds: {
          ...state.collapsedRepoIds,
          [repoId]: !state.collapsedRepoIds[repoId],
        },
      }));
    },

    openNewWorktree(repoId) {
      set({ draft: { repoId, name: "", baseRef: null, error: null } });
    },

    closeNewWorktree() {
      set({ draft: null });
    },

    /**
     * Closes the dialog at once and shows a progress row. If Git refuses, the
     * dialog reopens with what was typed and the reason, so nothing is lost.
     */
    async createWorktree(draft) {
      const pending: PendingWorktree = {
        id: `pending-${++nextPendingId}`,
        repoId: draft.repoId,
        name: draft.name.trim(),
      };
      set((state) => ({
        draft: null,
        pending: [...state.pending, pending],
        collapsedRepoIds: { ...state.collapsedRepoIds, [draft.repoId]: false },
      }));

      try {
        const created = await unwrap(
          commands.createWorktree(draft.repoId, draft.name, draft.baseRef),
        );
        await get().refreshWorktrees(draft.repoId);
        select(created.path);
      } catch (error) {
        const message = error instanceof Error ? error.message : String(error);
        set({ draft: { ...draft, error: message } });
      } finally {
        set((state) => ({
          pending: state.pending.filter((item) => item.id !== pending.id),
        }));
      }
    },

    async removeWorktree(repoId, path, force) {
      const result = await unwrap(commands.removeWorktree(repoId, path, force));
      await get().refreshWorktrees(repoId);
      reconcileSelection();
      return result;
    },
  };
});

// localStorage can be blocked or empty; the remembered selection is only a convenience.
function readRememberedSelection(): string | null {
  try {
    return localStorage.getItem(SELECTED_WORKTREE_KEY);
  } catch {
    return null;
  }
}

function rememberSelection(path: string | null) {
  try {
    if (path === null) localStorage.removeItem(SELECTED_WORKTREE_KEY);
    else localStorage.setItem(SELECTED_WORKTREE_KEY, path);
  } catch {
    // Not remembering is fine.
  }
}
