import { create } from "zustand";

/** One column with the removed lines inside it, or two columns side by side. */
export type DiffMode = "unified" | "split";

const KEY = "lynx-ide.diffMode";

// localStorage can be blocked; the choice is only a convenience.
function read(): DiffMode {
  try {
    return localStorage.getItem(KEY) === "split" ? "split" : "unified";
  } catch {
    return "unified";
  }
}

interface DiffModeState {
  mode: DiffMode;
  setMode: (mode: DiffMode) => void;
}

export const useDiffModeStore = create<DiffModeState>((set) => ({
  mode: read(),
  setMode(mode) {
    set({ mode });
    try {
      localStorage.setItem(KEY, mode);
    } catch {
      // Not remembering is fine.
    }
  },
}));
