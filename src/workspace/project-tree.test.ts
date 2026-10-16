import { describe, expect, it } from "vitest";

import type { Folder, Repo } from "@/bindings";
import {
  buildProjectTree,
  flattenFolders,
  folderAndDescendantIds,
  type ProjectNode,
} from "@/workspace/project-tree";

const folder = (
  id: string,
  name: string,
  parentId: string | null = null,
): Folder => ({
  id,
  name,
  parentId,
  collapsed: false,
});
const repo = (id: string, folderId: string | null = null): Repo => ({
  id,
  name: id,
  path: `/work/${id}`,
  folderId,
});

/** A compact view of the tree: folder names in brackets, repo ids bare. */
function shape(nodes: ProjectNode[]): string[] {
  return nodes.flatMap((node) =>
    node.kind === "repo"
      ? [node.repo.id]
      : [
          `[${node.folder.name}`,
          ...shape(node.children).map((line) => `  ${line}`),
          "]",
        ],
  );
}

describe("buildProjectTree", () => {
  it("lists folders first, sorted by name, then repos in the order they were added", () => {
    const tree = buildProjectTree(
      [folder("p", "Personal"), folder("w", "work")],
      [repo("zeta"), repo("alpha")],
    );
    expect(shape(tree)).toEqual([
      "[Personal",
      "]",
      "[work",
      "]",
      "zeta",
      "alpha",
    ]);
  });

  it("sorts folder names naturally and ignores case", () => {
    const tree = buildProjectTree(
      [folder("a", "Client 10"), folder("b", "client 2"), folder("c", "Alpha")],
      [],
    );
    expect(
      tree.map((node) => node.kind === "folder" && node.folder.name),
    ).toEqual(["Alpha", "client 2", "Client 10"]);
  });

  it("nests folders and repos under their parent folder", () => {
    const tree = buildProjectTree(
      [folder("work", "Work"), folder("x", "X", "work")],
      [repo("api", "x"), repo("site", "work"), repo("loose")],
    );
    expect(shape(tree)).toEqual([
      "[Work",
      "  [X",
      "    api",
      "  ]",
      "  site",
      "]",
      "loose",
    ]);
  });

  it("puts a folder or repo whose folder is missing at the top level", () => {
    const tree = buildProjectTree(
      [folder("orphan", "Orphan", "gone")],
      [repo("lost", "also-gone")],
    );
    expect(shape(tree)).toEqual(["[Orphan", "]", "lost"]);
  });

  it("keeps folders that form a loop, and what is inside them, visible", () => {
    const tree = buildProjectTree(
      [folder("a", "A", "b"), folder("b", "B", "a")],
      [repo("inside", "a")],
    );
    const names = shape(tree);
    expect(names).toContain("inside");
    expect(
      names.filter((line) => line.includes("A") || line.includes("B")),
    ).toHaveLength(2);
  });

  it("returns nothing for nothing", () => {
    expect(buildProjectTree([], [])).toEqual([]);
  });
});

describe("flattenFolders", () => {
  it("gives each folder its depth in tree order", () => {
    const flat = flattenFolders([
      folder("work", "Work"),
      folder("x", "X", "work"),
      folder("home", "Home"),
    ]);
    expect(flat.map(({ folder: f, depth }) => [f.name, depth])).toEqual([
      ["Home", 0],
      ["Work", 0],
      ["X", 1],
    ]);
  });
});

describe("folderAndDescendantIds", () => {
  const folders = [
    folder("work", "Work"),
    folder("x", "X", "work"),
    folder("deep", "Deep", "x"),
    folder("home", "Home"),
  ];

  it("includes the folder itself and everything inside it, at any depth", () => {
    expect([...folderAndDescendantIds(folders, "work")].sort()).toEqual([
      "deep",
      "work",
      "x",
    ]);
  });

  it("leaves siblings and parents out", () => {
    expect([...folderAndDescendantIds(folders, "x")].sort()).toEqual([
      "deep",
      "x",
    ]);
    expect(folderAndDescendantIds(folders, "home").has("work")).toBe(false);
  });
});
