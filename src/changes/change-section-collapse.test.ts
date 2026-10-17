import { beforeEach, describe, expect, it } from "vitest";

import { useCollapsedChangeSections } from "@/changes/change-section-collapse";

describe("useCollapsedChangeSections", () => {
  beforeEach(() => {
    useCollapsedChangeSections.setState({ collapsed: {} });
  });

  it("folds a section on the first toggle and opens it on the next", () => {
    const { toggle } = useCollapsedChangeSections.getState();

    toggle("changes");
    expect(useCollapsedChangeSections.getState().collapsed.changes).toBe(true);

    toggle("changes");
    expect(useCollapsedChangeSections.getState().collapsed.changes).toBe(false);
  });

  it("folds one section without touching the others", () => {
    useCollapsedChangeSections.getState().toggle("untracked");

    expect(useCollapsedChangeSections.getState().collapsed).toEqual({
      untracked: true,
    });
  });
});
