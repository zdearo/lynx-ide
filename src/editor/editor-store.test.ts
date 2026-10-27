import { beforeEach, describe, expect, it, vi } from "vitest";

import { commands } from "@/bindings";
import { useEditorStore } from "@/editor/editor-store";
import { failure, ok } from "@/test-support";
import { useTerminalStore } from "@/terminal/terminal-store";

vi.mock("@/bindings", () => ({
  commands: { readFile: vi.fn(), readFileImage: vi.fn(), writeFile: vi.fn() },
}));

const api = vi.mocked(commands);
const initialState = useEditorStore.getState();
const wt = "/work/api";

beforeEach(() => {
  vi.resetAllMocks();
  useEditorStore.setState(initialState, true);
  api.readFile.mockResolvedValue(ok({ content: "hello", hash: "h1" }));
});

const fileOf = (path: string) =>
  useEditorStore.getState().files[wt]?.find((file) => file.path === path);

describe("open", () => {
  it("reads the file, shows it and keeps its hash", async () => {
    await useEditorStore.getState().open(wt, "src/App.tsx");

    expect(fileOf("src/App.tsx")).toMatchObject({
      name: "App.tsx",
      status: "ready",
      text: "hello",
      hash: "h1",
      dirty: false,
    });
    expect(useEditorStore.getState().active[wt]).toBe("src/App.tsx");
  });

  it("opens a file once and just shows it the second time", async () => {
    await useEditorStore.getState().open(wt, "a.ts");
    await useEditorStore.getState().open(wt, "b.ts");
    await useEditorStore.getState().open(wt, "a.ts");

    expect(api.readFile).toHaveBeenCalledTimes(2);
    expect(useEditorStore.getState().files[wt]).toHaveLength(2);
    expect(useEditorStore.getState().active[wt]).toBe("a.ts");
  });

  it("keeps the failure kind when the file cannot be read as text", async () => {
    api.readFile.mockResolvedValue(failure("binaryFile", "not text"));
    await useEditorStore.getState().open(wt, "data.bin");

    expect(fileOf("data.bin")).toMatchObject({
      status: "error",
      errorKind: "binaryFile",
    });
  });
});

describe("images", () => {
  it("opens an image as a data URL, in the edit view, without reading it as text", async () => {
    api.readFileImage.mockResolvedValue(
      ok({ mime: "image/png", data: "AQID" }),
    );
    await useEditorStore
      .getState()
      .open(wt, "assets/logo.png", { view: "diff" });

    expect(api.readFile).not.toHaveBeenCalled();
    expect(fileOf("assets/logo.png")).toMatchObject({
      status: "ready",
      view: "edit",
      imageUrl: "data:image/png;base64,AQID",
    });
  });
});

describe("save", () => {
  it("sends the hash it read and takes the new one", async () => {
    await useEditorStore.getState().open(wt, "a.ts");
    useEditorStore.getState().markDirty(wt, "a.ts");
    api.writeFile.mockResolvedValue(ok({ content: "new", hash: "h2" }));

    const outcome = await useEditorStore.getState().save(wt, "a.ts", "new");

    expect(outcome).toBe("saved");
    expect(api.writeFile).toHaveBeenCalledWith(wt, "a.ts", "new", "h1");
    expect(fileOf("a.ts")).toMatchObject({ hash: "h2", dirty: false });
  });

  it("flags a conflict instead of overwriting", async () => {
    await useEditorStore.getState().open(wt, "a.ts");
    useEditorStore.getState().markDirty(wt, "a.ts");
    api.writeFile.mockResolvedValue(failure("fileChanged", "changed"));

    const outcome = await useEditorStore.getState().save(wt, "a.ts", "new");

    expect(outcome).toBe("conflict");
    expect(fileOf("a.ts")).toMatchObject({ conflict: true, dirty: true });
  });

  it("overwrites on purpose by sending no hash", async () => {
    await useEditorStore.getState().open(wt, "a.ts");
    api.writeFile.mockResolvedValue(ok({ content: "new", hash: "h3" }));

    await useEditorStore.getState().save(wt, "a.ts", "new", true);

    expect(api.writeFile).toHaveBeenCalledWith(wt, "a.ts", "new", null);
  });
});

describe("reload", () => {
  it("takes the disk text, clears the conflict and restarts the editor", async () => {
    await useEditorStore.getState().open(wt, "a.ts");
    useEditorStore.getState().markDirty(wt, "a.ts");
    api.writeFile.mockResolvedValue(failure("fileChanged", "changed"));
    await useEditorStore.getState().save(wt, "a.ts", "mine");
    const before = fileOf("a.ts")?.revision ?? 0;
    api.readFile.mockResolvedValue(ok({ content: "theirs", hash: "h9" }));

    await useEditorStore.getState().reload(wt, "a.ts");

    expect(fileOf("a.ts")).toMatchObject({
      text: "theirs",
      hash: "h9",
      dirty: false,
      conflict: false,
    });
    expect(fileOf("a.ts")?.revision).toBeGreaterThan(before);
  });
});

describe("close", () => {
  it("shows the neighbour of the file that closes", async () => {
    for (const name of ["a.ts", "b.ts", "c.ts"]) {
      await useEditorStore.getState().open(wt, name);
    }
    useEditorStore.getState().show(wt, "b.ts");

    useEditorStore.getState().close(wt, "b.ts");

    expect(useEditorStore.getState().active[wt]).toBe("c.ts");
  });

  it("goes back to the terminals when the last file closes", async () => {
    await useEditorStore.getState().open(wt, "a.ts");
    useEditorStore.getState().close(wt, "a.ts");

    expect(useEditorStore.getState().active[wt]).toBeNull();
  });
});

describe("terminals", () => {
  it("come back in front when another terminal is chosen", async () => {
    await useEditorStore.getState().open(wt, "a.ts");

    useTerminalStore.setState({ activeByWorktree: { [wt]: "t2" } });

    expect(useEditorStore.getState().active[wt]).toBeNull();
  });
});

describe("preview tabs", () => {
  const paths = () =>
    useEditorStore.getState().files[wt]?.map((file) => file.path);

  it("a new preview replaces the previous one in place", async () => {
    const store = useEditorStore.getState();
    await store.open(wt, "keep.ts");
    await store.open(wt, "a.ts", { preview: true });
    await store.open(wt, "b.ts", { preview: true });

    expect(paths()).toEqual(["keep.ts", "b.ts"]);
    expect(useEditorStore.getState().active[wt]).toBe("b.ts");
  });

  it("opening for good pins a preview tab", async () => {
    const store = useEditorStore.getState();
    await store.open(wt, "a.ts", { preview: true });
    await store.open(wt, "a.ts");

    expect(fileOf("a.ts")?.preview).toBe(false);
    await store.open(wt, "b.ts", { preview: true });
    expect(paths()).toEqual(["a.ts", "b.ts"]);
  });

  it("a preview open never demotes a permanent tab", async () => {
    const store = useEditorStore.getState();
    await store.open(wt, "a.ts");
    await store.open(wt, "a.ts", { preview: true });

    expect(fileOf("a.ts")?.preview).toBe(false);
  });

  it("typing in a preview tab makes it permanent", async () => {
    const store = useEditorStore.getState();
    await store.open(wt, "a.ts", { preview: true });
    store.markDirty(wt, "a.ts");

    expect(fileOf("a.ts")).toMatchObject({ preview: false, dirty: true });
  });

  it("remembers the view and the original path it was opened with", async () => {
    await useEditorStore.getState().open(wt, "new.ts", {
      preview: true,
      view: "diff",
      originalPath: "old.ts",
    });

    expect(fileOf("new.ts")).toMatchObject({
      view: "diff",
      originalPath: "old.ts",
    });

    useEditorStore.getState().setView(wt, "new.ts", "edit");
    expect(fileOf("new.ts")?.view).toBe("edit");
  });
});
