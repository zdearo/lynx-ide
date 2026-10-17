import { create } from "zustand";

export type LspServerState = "starting" | "indexing" | "ready" | "stopping";

/** A language server running for a worktree's open files. */
export interface LspServerStatus {
  language: string;
  state: LspServerState;
}

interface LspStatusState {
  servers: readonly LspServerStatus[];
}

/** Empty until the backend reports servers; the status bar reads it as it is. */
export const useLspStatusStore = create<LspStatusState>(() => ({
  servers: [],
}));
