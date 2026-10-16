import { create } from "zustand";

import {
  commands,
  type Folder,
  type RemoveWorktreeResult,
  type Repo,
  type WorktreeInfo,
} from "@/bindings";
import { IpcRequestError, unwrap } from "@/lib/ipc-result";

const SELECTED_WORKTREE_KEY = "lynx-ide.selectedWorktree";

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
  folders: Folder[];
  worktrees: Record<string, WorktreeInfo[]>;
  collapsedRepoIds: Record<string, boolean>;
  /** Worktrees whose agent list is open. Missing means collapsed, which is the default. */
  expandedAgentGroups: Record<string, boolean>;
  selectedWorktreePath: string | null;
  pending: PendingWorktree[];
  draft: WorktreeDraft | null;

  load: () => Promise<void>;
  /** Reloads repos and folders together, since a move changes both. */
  refreshStructure: () => Promise<void>;
  refreshAll: () => Promise<void>;
  createFolder: (name: string, parentId: string | null) => Promise<Folder>;
  renameFolder: (folderId: string, name: string) => Promise<Folder>;
  moveFolder: (folderId: string, parentId: string | null) => Promise<void>;
  deleteFolder: (folderId: string) => Promise<void>;
  setFolderCollapsed: (folderId: string, collapsed: boolean) => Promise<void>;
  moveRepo: (repoId: string, folderId: string | null) => Promise<void>;
  refreshWorktrees: (repoId: string) => Promise<void>;
  addRepo: (path: string) => Promise<Repo>;
  createProject: (parentPath: string, name: string) => Promise<Repo>;
  removeRepo: (repoId: string) => Promise<void>;
  selectWorktree: (path: string) => void;
  toggleRepo: (repoId: string) => void;
  toggleAgentGroup: (worktreePath: string) => void;
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

  /** Shows a repo the backend just registered, and selects its main worktree. */
  const registerRepo = async (repo: Repo) => {
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
  };

  return {
    loaded: false,
    repos: [],
    folders: [],
    worktrees: {},
    collapsedRepoIds: {},
    expandedAgentGroups: {},
    selectedWorktreePath: readRememberedSelection(),
    pending: [],
    draft: null,

    async load() {
      await get().refreshStructure();
      await get().refreshAll();
      set({ loaded: true });
    },

    async refreshStructure() {
      const [repos, folders] = await Promise.all([
        unwrap(commands.listRepos()),
        unwrap(commands.listFolders()),
      ]);
      set({ repos, folders });
    },

    async createFolder(name, parentId) {
      const folder = await unwrap(commands.createFolder(name, parentId));
      // A folder made inside a collapsed one would be hidden, so open its parent.
      if (parentId && get().folders.find((f) => f.id === parentId)?.collapsed) {
        await get().setFolderCollapsed(parentId, false);
      }
      await get().refreshStructure();
      return folder;
    },

    async renameFolder(folderId, name) {
      const folder = await unwrap(commands.renameFolder(folderId, name));
      await get().refreshStructure();
      return folder;
    },

    async moveFolder(folderId, parentId) {
      await unwrap(commands.moveFolder(folderId, parentId));
      await get().refreshStructure();
    },

    async deleteFolder(folderId) {
      await unwrap(commands.deleteFolder(folderId));
      await get().refreshStructure();
    },

    /** Optimistic: the sidebar toggles at once and is re-read if the backend refuses. */
    async setFolderCollapsed(folderId, collapsed) {
      set((state) => ({
        folders: state.folders.map((f) =>
          f.id === folderId ? { ...f, collapsed } : f,
        ),
      }));
      try {
        await unwrap(commands.setFolderCollapsed(folderId, collapsed));
      } catch (error) {
        await get().refreshStructure();
        throw error;
      }
    },

    async moveRepo(repoId, folderId) {
      await unwrap(commands.moveRepo(repoId, folderId));
      await get().refreshStructure();
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
      return registerRepo(await unwrap(commands.addRepo(path)));
    },

    async createProject(parentPath, name) {
      return registerRepo(
        await unwrap(commands.createProject(parentPath, name)),
      );
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

    toggleAgentGroup(worktreePath) {
      set((state) => ({
        expandedAgentGroups: {
          ...state.expandedAgentGroups,
          [worktreePath]: !state.expandedAgentGroups[worktreePath],
        },
      }));
    },

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
