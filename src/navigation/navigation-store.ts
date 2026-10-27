import { create } from "zustand";

/** The views the primary bar can show in the side panel. */
export type NavigationViewId = "projects" | "bots";

const ACTIVE_VIEW_KEY = "lynx-ide.activeView";
const PANEL_COLLAPSED_KEY = "lynx-ide.panelCollapsed";

interface NavigationState {
  activeView: NavigationViewId;
  /** Wide layout: the side panel is hidden and only the primary bar remains. */
  panelCollapsed: boolean;
  /** Narrow layout: the primary bar and side panel slide in over the terminal. */
  drawerOpen: boolean;
  settingsOpen: boolean;
  /** The settings section to open on, when something elsewhere links to one. Null: the first. */
  settingsSection: string | null;

  /** Clicking the active view again hides the panel, like the VS Code activity bar. */
  selectView: (view: NavigationViewId, wide: boolean) => void;
  togglePanel: (wide: boolean) => void;
  setDrawerOpen: (open: boolean) => void;
  openSettings: (section?: string) => void;
  closeSettings: () => void;
}

export const useNavigationStore = create<NavigationState>((set, get) => ({
  activeView: "projects",
  panelCollapsed: readFlag(PANEL_COLLAPSED_KEY),
  drawerOpen: false,
  settingsOpen: false,
  settingsSection: null,

  selectView(view, wide) {
    const { activeView, panelCollapsed } = get();
    if (!wide) {
      set({ activeView: view });
    } else if (view === activeView && !panelCollapsed) {
      set({ panelCollapsed: true });
    } else {
      set({ activeView: view, panelCollapsed: false });
    }
    remember(ACTIVE_VIEW_KEY, get().activeView);
    remember(PANEL_COLLAPSED_KEY, String(get().panelCollapsed));
  },

  togglePanel(wide) {
    if (wide) {
      set((state) => ({ panelCollapsed: !state.panelCollapsed }));
      remember(PANEL_COLLAPSED_KEY, String(get().panelCollapsed));
    } else {
      set((state) => ({ drawerOpen: !state.drawerOpen }));
    }
  },

  setDrawerOpen: (drawerOpen) => set({ drawerOpen }),
  openSettings: (section) =>
    set({ settingsOpen: true, settingsSection: section ?? null }),
  closeSettings: () => set({ settingsOpen: false, settingsSection: null }),
}));

// localStorage can be blocked; these are conveniences, not state anyone depends on.
function readFlag(key: string): boolean {
  try {
    return localStorage.getItem(key) === "true";
  } catch {
    return false;
  }
}

function remember(key: string, value: string) {
  try {
    localStorage.setItem(key, value);
  } catch {
    // Not remembering is fine.
  }
}
