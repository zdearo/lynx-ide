import { beforeEach, describe, expect, it } from "vitest";

import { useRightPanelStore } from "@/right-panel/right-panel-store";

const initialState = useRightPanelStore.getState();

beforeEach(() => {
  useRightPanelStore.setState(initialState, true);
});

describe("toggle", () => {
  it("flips the docked panel when the window is wide enough", () => {
    useRightPanelStore.getState().toggle(true);
    expect(useRightPanelStore.getState().dockedOpen).toBe(true);
    useRightPanelStore.getState().toggle(true);
    expect(useRightPanelStore.getState().dockedOpen).toBe(false);
  });

  it("flips the drawer instead when it is not, leaving the docked choice alone", () => {
    useRightPanelStore.getState().toggle(false);
    expect(useRightPanelStore.getState().drawerOpen).toBe(true);
    expect(useRightPanelStore.getState().dockedOpen).toBe(false);
  });
});

describe("show", () => {
  it("opens the panel on a tab, docked or as a drawer", () => {
    useRightPanelStore.getState().show("files", true);
    expect(useRightPanelStore.getState().dockedOpen).toBe(true);

    useRightPanelStore.setState({ dockedOpen: false });
    useRightPanelStore.getState().show("files", false);
    expect(useRightPanelStore.getState().drawerOpen).toBe(true);
    expect(useRightPanelStore.getState().activeTab).toBe("files");
  });
});
