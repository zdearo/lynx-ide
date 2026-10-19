import { describe, expect, it } from "vitest";

import type { TreeState } from "@/files/file-tree-rows";
import { directoriesToReload } from "@/watching/worktree-change";

const listing = { status: "ready", entries: [], truncated: false, error: null };
const tree: TreeState = {
  directories: { "": listing, src: listing, "src/ui": listing },
  expanded: { src: true },
  selected: null,
} as TreeState;

describe("directoriesToReload", () => {
  it("lists again only the changed folders the explorer has listed", () => {
    expect(
      directoriesToReload(tree, {
        directories: ["src", "docs", "src/ui/deep"],
        everything: false,
      }),
    ).toEqual(["src"]);
  });

  it("reads every listed folder when too much changed to name", () => {
    expect(
      directoriesToReload(tree, { directories: [], everything: true }),
    ).toEqual(["", "src", "src/ui"]);
  });

  it("does nothing for a worktree the explorer never opened", () => {
    expect(
      directoriesToReload(undefined, { directories: [""], everything: false }),
    ).toEqual([]);
  });
});
