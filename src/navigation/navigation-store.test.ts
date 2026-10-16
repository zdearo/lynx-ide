import { beforeEach, describe, expect, it } from "vitest";

import { useNavigationStore } from "@/navigation/navigation-store";

const initialState = useNavigationStore.getState();

beforeEach(() => {
  useNavigationStore.setState(initialState, true);
});

describe("selectView", () => {
  it("hides the panel when the active view is clicked again on a wide window", () => {
    useNavigationStore.getState().selectView("projects", true);
    expect(useNavigationStore.getState().panelCollapsed).toBe(true);
  });

  it("shows the panel again on the next click", () => {
    useNavigationStore.setState({ panelCollapsed: true });
    useNavigationStore.getState().selectView("projects", true);
    expect(useNavigationStore.getState().panelCollapsed).toBe(false);
  });

  it("never collapses anything on a narrow window, where the panel is in a drawer", () => {
    useNavigationStore.setState({ drawerOpen: true });
    useNavigationStore.getState().selectView("projects", false);
    const state = useNavigationStore.getState();
    expect(state.drawerOpen).toBe(true);
    expect(state.panelCollapsed).toBe(false);
  });
});

describe("togglePanel", () => {
  it("collapses and expands the panel on a wide window", () => {
    useNavigationStore.getState().togglePanel(true);
    expect(useNavigationStore.getState().panelCollapsed).toBe(true);
    useNavigationStore.getState().togglePanel(true);
    expect(useNavigationStore.getState().panelCollapsed).toBe(false);
  });

  it("opens and closes the drawer on a narrow window and leaves the panel alone", () => {
    useNavigationStore.getState().togglePanel(false);
    expect(useNavigationStore.getState().drawerOpen).toBe(true);
    useNavigationStore.getState().togglePanel(false);
    const state = useNavigationStore.getState();
    expect(state.drawerOpen).toBe(false);
    expect(state.panelCollapsed).toBe(false);
  });
});

describe("settings", () => {
  it("opens and closes", () => {
    useNavigationStore.getState().openSettings();
    expect(useNavigationStore.getState().settingsOpen).toBe(true);
    useNavigationStore.getState().closeSettings();
    expect(useNavigationStore.getState().settingsOpen).toBe(false);
  });
});
