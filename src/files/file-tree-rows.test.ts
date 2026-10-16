import { describe, expect, it } from "vitest";

import type { FileEntry } from "@/bindings";
import {
  visibleRows,
  type DirectoryState,
  type TreeState,
} from "@/files/file-tree-rows";

const dir = (name: string, extra: Partial<FileEntry> = {}): FileEntry => ({
  name,
  kind: "directory",
  symlink: false,
  ignored: false,
  ...extra,
});
const file = (name: string, extra: Partial<FileEntry> = {}): FileEntry => ({
  name,
  kind: "file",
  symlink: false,
  ignored: false,
  ...extra,
});
const ready = (
  entries: FileEntry[],
  extra: Partial<DirectoryState> = {},
): DirectoryState => ({
  status: "ready",
  entries,
  truncated: false,
  error: null,
  ...extra,
});

function tree(partial: Partial<TreeState>): TreeState {
  return { directories: {}, expanded: {}, selected: null, ...partial };
}

describe("visibleRows", () => {
  it("is empty before anything has loaded", () => {
    expect(visibleRows(tree({}))).toEqual([]);
  });

  it("lists the root and keeps closed folders closed", () => {
    const rows = visibleRows(
      tree({
        directories: {
          "": ready([dir("src"), file("README.md")]),
          src: ready([file("main.rs")]),
        },
      }),
    );
    expect(rows.map((r) => r.path)).toEqual(["src", "README.md"]);
    expect(rows[0]).toMatchObject({
      isDirectory: true,
      expanded: false,
      depth: 0,
    });
  });

  it("puts the children of an open folder right under it, one level deeper", () => {
    const rows = visibleRows(
      tree({
        directories: {
          "": ready([dir("src"), file("README.md")]),
          src: ready([dir("ui"), file("main.rs")]),
          "src/ui": ready([file("Button.tsx")]),
        },
        expanded: { src: true, "src/ui": true },
      }),
    );
    expect(
      rows.map((r) => [r.path, r.type === "entry" ? r.depth : r.depth]),
    ).toEqual([
      ["src", 0],
      ["src/ui", 1],
      ["src/ui/Button.tsx", 2],
      ["src/main.rs", 1],
      ["README.md", 0],
    ]);
  });

  it("marks a folder as loading while its first listing is on the way", () => {
    const rows = visibleRows(
      tree({
        directories: {
          "": ready([dir("src")]),
          src: {
            status: "loading",
            entries: [],
            truncated: false,
            error: null,
          },
        },
        expanded: { src: true },
      }),
    );
    expect(rows).toHaveLength(1);
    expect(rows[0]).toMatchObject({ loading: true, expanded: true });
  });

  it("explains an empty folder, a failed one and a cut one", () => {
    const rows = visibleRows(
      tree({
        directories: {
          "": ready([dir("empty"), dir("broken"), dir("huge")]),
          empty: ready([]),
          broken: {
            status: "error",
            entries: [],
            truncated: false,
            error: "permission denied",
          },
          huge: ready([file("a")], { truncated: true }),
        },
        expanded: { empty: true, broken: true, huge: true },
      }),
    );
    const notes = rows.filter((r) => r.type === "note");
    expect(notes.map((n) => n.type === "note" && n.kind)).toEqual([
      "empty",
      "error",
      "truncated",
    ]);
    expect(
      notes.find((n) => n.type === "note" && n.kind === "error"),
    ).toMatchObject({
      text: "permission denied",
    });
  });

  it("keeps the last entries visible while a refresh is loading", () => {
    const rows = visibleRows(
      tree({
        directories: {
          "": {
            status: "loading",
            entries: [file("a.txt")],
            truncated: false,
            error: null,
          },
        },
      }),
    );
    expect(rows.map((r) => r.path)).toEqual(["a.txt"]);
  });

  it("carries ignored and symlink flags to the row", () => {
    const rows = visibleRows(
      tree({
        directories: {
          "": ready([
            dir("node_modules", { ignored: true }),
            file("link", { symlink: true }),
          ]),
        },
      }),
    );
    expect(rows[0]).toMatchObject({ ignored: true });
    expect(rows[1]).toMatchObject({ symlink: true });
  });

  it("gives every row a unique key", () => {
    const rows = visibleRows(
      tree({
        directories: {
          "": ready([dir("a"), dir("b")]),
          a: ready([]),
          b: ready([]),
        },
        expanded: { a: true, b: true },
      }),
    );
    const paths = rows.map((r) => r.path);
    expect(new Set(paths).size).toBe(paths.length);
  });
});
