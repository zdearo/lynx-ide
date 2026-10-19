import { create } from "zustand";

import type { Folder, Repo, WorktreeInfo } from "@/bindings";

/** What a move is about: a repo or a folder, with the object itself so the dialog can show it. */
export type MoveTarget =
  { kind: "repo"; repo: Repo } | { kind: "folder"; folder: Folder };

/** Create a folder inside `parentId` (null: top level), or rename an existing one. */
export type FolderDialog =
  | { mode: "create"; parentId: string | null }
  | { mode: "rename"; folder: Folder };

/** Which confirmation or prompt dialog is open, and for what. */
interface DialogState {
  /** Worktree path that a custom command will run in. */
  customCommandFor: string | null;
  removeWorktreeTarget: { repoId: string; worktree: WorktreeInfo } | null;
  removeRepoTarget: Repo | null;
  folderDialog: FolderDialog | null;
  moveTarget: MoveTarget | null;
  deleteFolderTarget: Folder | null;
  addProjectOpen: boolean;
  /** Terminal session the user is asked to confirm closing. */
  closeTerminalId: string | null;
  /** A file with unsaved changes the user is asked to confirm closing. */
  closeFileTarget: { worktreePath: string; path: string } | null;
  /** Worktree whose repository gets a GitHub remote, once the user confirms. */
  createRemoteFor: string | null;

  askCustomCommand: (worktreePath: string | null) => void;
  askRemoveWorktree: (
    target: { repoId: string; worktree: WorktreeInfo } | null,
  ) => void;
  askRemoveRepo: (repo: Repo | null) => void;
  askFolderDialog: (dialog: FolderDialog | null) => void;
  askMove: (target: MoveTarget | null) => void;
  askDeleteFolder: (folder: Folder | null) => void;
  askAddProject: (open: boolean) => void;
  askCloseTerminal: (id: string | null) => void;
  askCloseFile: (target: { worktreePath: string; path: string } | null) => void;
  askCreateRemote: (worktreePath: string | null) => void;
}

export const useDialogStore = create<DialogState>((set) => ({
  customCommandFor: null,
  removeWorktreeTarget: null,
  removeRepoTarget: null,
  folderDialog: null,
  moveTarget: null,
  deleteFolderTarget: null,
  addProjectOpen: false,
  closeTerminalId: null,
  closeFileTarget: null,
  createRemoteFor: null,

  askCustomCommand: (customCommandFor) => set({ customCommandFor }),
  askRemoveWorktree: (removeWorktreeTarget) => set({ removeWorktreeTarget }),
  askRemoveRepo: (removeRepoTarget) => set({ removeRepoTarget }),
  askFolderDialog: (folderDialog) => set({ folderDialog }),
  askMove: (moveTarget) => set({ moveTarget }),
  askDeleteFolder: (deleteFolderTarget) => set({ deleteFolderTarget }),
  askAddProject: (addProjectOpen) => set({ addProjectOpen }),
  askCloseTerminal: (closeTerminalId) => set({ closeTerminalId }),
  askCloseFile: (closeFileTarget) => set({ closeFileTarget }),
  askCreateRemote: (createRemoteFor) => set({ createRemoteFor }),
}));
