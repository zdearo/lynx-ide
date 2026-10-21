import { create } from "zustand";

/** The tabs of the right-hand panel. */
export type RightPanelTabId = "files" | "changes" | "pull-request" | "browser";

const DOCKED_OPEN_KEY = "lynx-ide.rightPanelOpen";

interface RightPanelState {
  activeTab: RightPanelTabId;
  /** Wide window: the panel sits beside the terminal. Remembered between sessions. */
  dockedOpen: boolean;
  /** Narrower window: the panel slides over the terminal. Always starts closed. */
  drawerOpen: boolean;

  selectTab: (tab: RightPanelTabId) => void;
  toggle: (canDock: boolean) => void;
  /** Opens the panel on `tab`, whichever way it is shown. */
  show: (tab: RightPanelTabId, canDock: boolean) => void;
  setDrawerOpen: (open: boolean) => void;
}

export const useRightPanelStore = create<RightPanelState>((set, get) => ({
  activeTab: "files",
  dockedOpen: readOpen(),
  drawerOpen: false,

  selectTab: (activeTab) => set({ activeTab }),

  toggle(canDock) {
    if (canDock) {
      set((state) => ({ dockedOpen: !state.dockedOpen }));
      remember(get().dockedOpen);
    } else {
      set((state) => ({ drawerOpen: !state.drawerOpen }));
    }
  },

  show(tab, canDock) {
    if (canDock) {
      set({ activeTab: tab, dockedOpen: true });
      remember(true);
    } else {
      set({ activeTab: tab, drawerOpen: true });
    }
  },

  setDrawerOpen: (drawerOpen) => set({ drawerOpen }),
}));

// localStorage can be blocked; whether the panel was open is only a convenience.
function readOpen(): boolean {
  try {
    return localStorage.getItem(DOCKED_OPEN_KEY) === "true";
  } catch {
    return false;
  }
}

function remember(open: boolean) {
  try {
    localStorage.setItem(DOCKED_OPEN_KEY, String(open));
  } catch {
    // Not remembering is fine.
  }
}
