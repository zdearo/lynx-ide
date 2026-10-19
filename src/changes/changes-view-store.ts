import { create } from "zustand";

/** What the Changes tab lists under the commit box: the files that changed, or the commits made. */
export type ChangesView = "changes" | "commits";

interface ChangesViewState {
  view: ChangesView;
  setView: (view: ChangesView) => void;
}

export const useChangesViewStore = create<ChangesViewState>((set) => ({
  view: "changes",
  setView: (view) => set({ view }),
}));
