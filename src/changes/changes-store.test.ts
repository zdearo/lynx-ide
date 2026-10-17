import { beforeEach, describe, expect, it, vi } from "vitest";

import { commands, type ChangedFile } from "@/bindings";
import { changeOf, useChangesStore } from "@/changes/changes-store";
import { failure, ok } from "@/test-support";

vi.mock("@/bindings", () => ({ commands: { listChanges: vi.fn() } }));

const api = vi.mocked(commands);
const initialState = useChangesStore.getState();
const wt = "/work/api";

const modified = (path: string): ChangedFile => ({
  path,
  originalPath: null,
  staged: null,
  unstaged: "modified",
});

beforeEach(() => {
  vi.resetAllMocks();
  useChangesStore.setState(initialState, true);
});

describe("refresh", () => {
  it("stores the files and counts the read", async () => {
    api.listChanges.mockResolvedValue(ok([modified("a.ts")]));

    await useChangesStore.getState().refresh(wt);

    const changes = useChangesStore.getState().byWorktree[wt];
    expect(changes).toMatchObject({ status: "ready", tick: 1 });
    expect(changeOf(changes, "a.ts")).toBeDefined();
    expect(changeOf(changes, "b.ts")).toBeUndefined();
  });

  it("keeps the same list object when nothing changed", async () => {
    api.listChanges.mockImplementation(async () => ok([modified("a.ts")]));
    await useChangesStore.getState().refresh(wt);
    const first = useChangesStore.getState().byWorktree[wt].files;

    await useChangesStore.getState().refresh(wt);

    const second = useChangesStore.getState().byWorktree[wt];
    expect(second.files).toBe(first);
    expect(second.tick).toBe(2);
  });

  it("keeps the last list when a later read fails", async () => {
    api.listChanges.mockResolvedValueOnce(ok([modified("a.ts")]));
    await useChangesStore.getState().refresh(wt);
    api.listChanges.mockResolvedValueOnce(failure("git", "boom"));

    await useChangesStore.getState().refresh(wt);

    const changes = useChangesStore.getState().byWorktree[wt];
    expect(changes.files).toHaveLength(1);
    expect(changes.status).toBe("ready");
    expect(changes.error).toContain("boom");
  });

  it("reports the failure when there is no list to keep", async () => {
    api.listChanges.mockResolvedValue(failure("notAGitRepo", "not a repo"));

    await useChangesStore.getState().refresh(wt);

    expect(useChangesStore.getState().byWorktree[wt]).toMatchObject({
      status: "error",
    });
  });
});
