import { beforeEach, describe, expect, it, vi } from "vitest";

const refreshes = vi.hoisted(() => ({
  changes: vi.fn<(path: string) => Promise<void>>(),
  sync: vi.fn<(path: string) => Promise<void>>(),
  commits: vi.fn<(path: string) => Promise<void>>(),
}));

vi.mock("@/changes/changes-store", () => ({
  useChangesStore: { getState: () => ({ refresh: refreshes.changes }) },
}));
vi.mock("@/changes/branch-sync-store", () => ({
  useBranchSyncStore: { getState: () => ({ refresh: refreshes.sync }) },
}));
vi.mock("@/changes/commits/commits-store", () => ({
  useCommitsStore: { getState: () => ({ refresh: refreshes.commits }) },
}));

import { refreshWorktreeGit } from "@/changes/refresh-worktree-git";

function slow() {
  let finish: () => void = () => {};
  const promise = new Promise<void>((resolve) => (finish = resolve));
  return { promise, finish };
}

beforeEach(() => {
  for (const refresh of Object.values(refreshes))
    refresh.mockReset().mockResolvedValue();
});

describe("refreshWorktreeGit", () => {
  it("reads the three stores", async () => {
    await refreshWorktreeGit("/a");
    expect(refreshes.changes).toHaveBeenCalledWith("/a");
    expect(refreshes.sync).toHaveBeenCalledWith("/a");
    expect(refreshes.commits).toHaveBeenCalledWith("/a");
  });

  it("turns asks made during a read into one more read, not one each", async () => {
    const first = slow();
    refreshes.changes.mockReturnValueOnce(first.promise);
    const all = [
      refreshWorktreeGit("/a"),
      refreshWorktreeGit("/a"),
      refreshWorktreeGit("/a"),
      refreshWorktreeGit("/a"),
    ];
    expect(refreshes.changes).toHaveBeenCalledTimes(1);
    first.finish();
    await Promise.all(all);
    expect(refreshes.changes).toHaveBeenCalledTimes(2);
  });

  it("does not make worktrees wait for each other", async () => {
    const stuck = slow();
    refreshes.changes.mockReturnValueOnce(stuck.promise);
    const a = refreshWorktreeGit("/a");
    await refreshWorktreeGit("/b");
    expect(refreshes.changes).toHaveBeenCalledWith("/b");
    stuck.finish();
    await a;
  });

  it("can read again once a read is over, even after a failure", async () => {
    refreshes.changes.mockRejectedValueOnce(new Error("git failed"));
    await expect(refreshWorktreeGit("/a")).rejects.toThrow("git failed");
    await refreshWorktreeGit("/a");
    expect(refreshes.changes).toHaveBeenCalledTimes(2);
  });
});
