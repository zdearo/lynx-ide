import { beforeEach, describe, expect, it, vi } from "vitest";

import { commands, type ResumableSession } from "@/bindings";
import { failure, ok } from "@/test-support";
import { useResumableStore } from "@/workspace/resumable-store";

vi.mock("@/bindings", () => ({
  commands: {
    listResumableSessions: vi.fn(),
    forgetResumableSession: vi.fn(),
  },
}));

const api = vi.mocked(commands);
const initialState = useResumableStore.getState();
const wt = "/work/api";

const ended = (id: string): ResumableSession => ({
  id,
  harness: "claude",
  resumeArguments: ["--resume", "conv"],
  endedAt: 1,
});

beforeEach(() => {
  vi.resetAllMocks();
  useResumableStore.setState(initialState, true);
});

describe("remove", () => {
  it("takes the session off the list once the backend has removed it", async () => {
    useResumableStore.setState({
      byWorktree: { [wt]: [ended("a"), ended("b")] },
    });
    api.forgetResumableSession.mockResolvedValue(ok(null));

    await useResumableStore.getState().remove(wt, "a");

    expect(api.forgetResumableSession).toHaveBeenCalledWith("a");
    expect(useResumableStore.getState().byWorktree[wt]).toEqual([ended("b")]);
  });

  it("keeps the session listed and reports the failure when the backend refuses", async () => {
    useResumableStore.setState({ byWorktree: { [wt]: [ended("a")] } });
    api.forgetResumableSession.mockResolvedValue(
      failure("terminal", "the list is locked"),
    );

    await expect(
      useResumableStore.getState().remove(wt, "a"),
    ).rejects.toThrow();

    expect(useResumableStore.getState().byWorktree[wt]).toEqual([ended("a")]);
  });
});
