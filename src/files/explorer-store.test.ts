import { beforeEach, describe, expect, it, vi } from "vitest";

import { commands, type FileEntry } from "@/bindings";
import { useExplorerStore } from "@/files/explorer-store";
import { failure, ok } from "@/test-support";

vi.mock("@/bindings", () => ({ commands: { listDirectory: vi.fn() } }));

const api = vi.mocked(commands);
const initialState = useExplorerStore.getState();
const wt = "/work/api";

const file = (name: string): FileEntry => ({
  name,
  kind: "file",
  symlink: false,
  ignored: false,
});
const dir = (name: string): FileEntry => ({
  name,
  kind: "directory",
  symlink: false,
  ignored: false,
});
const listing = (entries: FileEntry[], truncated = false) =>
  ok({ entries, truncated });

function listsBy(map: Record<string, FileEntry[]>) {
  api.listDirectory.mockImplementation(
    async (_path: string, directory: string) => listing(map[directory] ?? []),
  );
}

beforeEach(() => {
  vi.resetAllMocks();
  useExplorerStore.setState(initialState, true);
});

describe("load", () => {
  it("stores the entries of a folder", async () => {
    listsBy({ "": [dir("src"), file("a.txt")] });

    await useExplorerStore.getState().load(wt, "");

    const root = useExplorerStore.getState().trees[wt].directories[""];
    expect(root.status).toBe("ready");
    expect(root.entries.map((e) => e.name)).toEqual(["src", "a.txt"]);
    expect(api.listDirectory).toHaveBeenCalledWith(wt, "");
  });

  it("keeps a listed folder ready and its entries on screen while reloading", async () => {
    listsBy({ "": [file("a.txt")] });
    await useExplorerStore.getState().load(wt, "");
    let finish: (value: ReturnType<typeof listing>) => void = () => {};
    api.listDirectory.mockReturnValue(
      new Promise((resolve) => {
        finish = resolve as typeof finish;
      }),
    );

    const reloading = useExplorerStore.getState().load(wt, "");

    const during = useExplorerStore.getState().trees[wt].directories[""];
    // A folder already on screen reloads quietly: it never flips to "loading".
    expect(during.status).toBe("ready");
    expect(during.entries.map((e) => e.name)).toEqual(["a.txt"]);

    finish(listing([file("b.txt")]));
    await reloading;
    expect(
      useExplorerStore
        .getState()
        .trees[wt].directories[""].entries.map((e) => e.name),
    ).toEqual(["b.txt"]);
  });

  it("leaves the folder untouched when the listing did not change", async () => {
    listsBy({ "": [file("a.txt"), dir("src")] });
    await useExplorerStore.getState().load(wt, "");
    const before = useExplorerStore.getState().trees[wt];

    await useExplorerStore.getState().load(wt, "");

    // The same object: nothing subscribed to the tree redraws.
    expect(useExplorerStore.getState().trees[wt]).toBe(before);
  });

  it("shows loading the first time a folder is listed", async () => {
    let finish: (value: ReturnType<typeof listing>) => void = () => {};
    api.listDirectory.mockReturnValue(
      new Promise((resolve) => {
        finish = resolve as typeof finish;
      }),
    );

    const first = useExplorerStore.getState().load(wt, "");
    expect(useExplorerStore.getState().trees[wt].directories[""].status).toBe(
      "loading",
    );

    finish(listing([file("a.txt")]));
    await first;
  });

  it("records the reason and keeps the entries when listing fails", async () => {
    listsBy({ "": [file("a.txt")] });
    await useExplorerStore.getState().load(wt, "");
    api.listDirectory.mockResolvedValue(
      failure("fileSystem", "could not read: permission denied"),
    );

    await useExplorerStore.getState().load(wt, "");

    const root = useExplorerStore.getState().trees[wt].directories[""];
    expect(root.status).toBe("error");
    expect(root.error).toContain("permission denied");
    expect(root.entries).toHaveLength(1);
  });

  it("closes a folder that disappeared from disk instead of showing an error", async () => {
    listsBy({ "": [dir("gone")], gone: [file("x")] });
    await useExplorerStore.getState().load(wt, "");
    await useExplorerStore.getState().toggle(wt, "gone");
    api.listDirectory.mockResolvedValue(
      failure("pathNotFound", "gone does not exist"),
    );

    await useExplorerStore.getState().load(wt, "gone");

    const tree = useExplorerStore.getState().trees[wt];
    expect(tree.expanded.gone).toBeUndefined();
    expect(tree.directories.gone).toBeUndefined();
  });
});

describe("toggle", () => {
  it("opens a folder and loads it once, then closes and reopens without reloading", async () => {
    listsBy({ "": [dir("src")], src: [file("main.rs")] });
    await useExplorerStore.getState().load(wt, "");
    api.listDirectory.mockClear();

    await useExplorerStore.getState().toggle(wt, "src");
    expect(useExplorerStore.getState().trees[wt].expanded.src).toBe(true);
    expect(api.listDirectory).toHaveBeenCalledTimes(1);

    await useExplorerStore.getState().toggle(wt, "src");
    expect(useExplorerStore.getState().trees[wt].expanded.src).toBeUndefined();

    await useExplorerStore.getState().toggle(wt, "src");
    expect(api.listDirectory).toHaveBeenCalledTimes(1);
  });
});

describe("refresh and collapse", () => {
  it("reloads the root and every open folder", async () => {
    listsBy({ "": [dir("src"), dir("docs")], src: [dir("ui")], "src/ui": [] });
    await useExplorerStore.getState().load(wt, "");
    await useExplorerStore.getState().toggle(wt, "src");
    await useExplorerStore.getState().toggle(wt, "src/ui");
    api.listDirectory.mockClear();
    listsBy({
      "": [dir("src")],
      src: [dir("ui")],
      "src/ui": [file("new.tsx")],
    });

    await useExplorerStore.getState().refresh(wt);

    expect(api.listDirectory.mock.calls.map((c) => c[1]).sort()).toEqual([
      "",
      "src",
      "src/ui",
    ]);
    expect(
      useExplorerStore.getState().trees[wt].directories["src/ui"].entries,
    ).toHaveLength(1);
  });

  it("collapses everything but keeps what was loaded", async () => {
    listsBy({ "": [dir("src")], src: [file("a")] });
    await useExplorerStore.getState().load(wt, "");
    await useExplorerStore.getState().toggle(wt, "src");

    useExplorerStore.getState().collapseAll(wt);

    const tree = useExplorerStore.getState().trees[wt];
    expect(tree.expanded).toEqual({});
    expect(tree.directories.src.entries).toHaveLength(1);
  });

  it("keeps worktrees apart", async () => {
    listsBy({ "": [file("a.txt")] });
    await useExplorerStore.getState().load("/work/one", "");

    expect(useExplorerStore.getState().trees["/work/two"]).toBeUndefined();
  });
});

describe("select", () => {
  it("remembers the selection per worktree", () => {
    useExplorerStore.getState().select(wt, "src/main.rs");
    expect(useExplorerStore.getState().trees[wt].selected).toBe("src/main.rs");
    useExplorerStore.getState().select(wt, null);
    expect(useExplorerStore.getState().trees[wt].selected).toBeNull();
  });
});
