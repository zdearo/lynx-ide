import { describe, expect, it } from "vitest";

import type { Folder, Repo } from "@/bindings";
import { visibleFirst } from "@/workspace/projects/repo-load-order";

const folder = (
  id: string,
  parentId: string | null,
  collapsed: boolean,
): Folder => ({
  id,
  name: id,
  parentId,
  collapsed,
  sourcePath: null,
  linkedRootId: null,
  missing: false,
});
const repo = (id: string, folderId: string | null): Repo => ({
  id,
  name: id,
  path: `/w/${id}`,
  folderId,
  linkedRootId: null,
  missing: false,
});

describe("visibleFirst", () => {
  it("puts repos inside a collapsed folder, at any depth, after the visible ones", () => {
    const folders = [
      folder("open", null, false),
      folder("shut", null, true),
      folder("inner", "shut", false),
    ];
    const repos = [
      repo("a", "inner"),
      repo("b", null),
      repo("c", "shut"),
      repo("d", "open"),
    ];

    expect(visibleFirst(repos, folders).map((r) => r.id)).toEqual([
      "b",
      "d",
      "a",
      "c",
    ]);
  });

  it("keeps every repo when a folder is missing or the data loops", () => {
    const folders = [folder("a", "b", false), folder("b", "a", false)];
    const repos = [repo("x", "a"), repo("y", "ghost")];

    expect(visibleFirst(repos, folders)).toHaveLength(2);
  });
});
