import { create } from "zustand";

interface WatchState {
  /** The worktree the backend is watching, or null when it is watching none. */
  watched: string | null;
  setWatched: (path: string | null) => void;
}

export const useWatchStore = create<WatchState>((set) => ({
  watched: null,
  setWatched: (watched) => set({ watched }),
}));
