import { beforeEach, describe, expect, it, vi } from "vitest";

import { commands } from "@/bindings";
import { failure, ok, session } from "@/test-support";
import { activeSessionId, useTerminalStore } from "@/terminal/terminal-store";

vi.mock("@/bindings", () => ({
  commands: {
    listTerminals: vi.fn(),
    createTerminal: vi.fn(),
    closeTerminal: vi.fn(),
  },
}));

const api = vi.mocked(commands);
const initialState = useTerminalStore.getState();
const wt = "/work/api";

beforeEach(() => {
  vi.resetAllMocks();
  api.closeTerminal.mockResolvedValue(ok(null));
  useTerminalStore.setState(initialState, true);
});

describe("create", () => {
  it("adds the session and makes it the active tab of its worktree", async () => {
    api.createTerminal.mockResolvedValue(ok(session("t1", wt)));

    await useTerminalStore.getState().create(wt, "claude");

    const state = useTerminalStore.getState();
    expect(state.sessions.map((s) => s.id)).toEqual(["t1"]);
    expect(state.activeByWorktree[wt]).toBe("t1");
    expect(api.createTerminal).toHaveBeenCalledWith(
      wt,
      "claude",
      expect.any(Number),
      expect.any(Number),
    );
  });

  it("changes nothing when the backend refuses", async () => {
    api.createTerminal.mockResolvedValue(
      failure("terminal", "no such directory"),
    );

    await expect(
      useTerminalStore.getState().create(wt, null),
    ).rejects.toMatchObject({
      kind: "terminal",
    });
    expect(useTerminalStore.getState().sessions).toEqual([]);
  });
});

describe("close", () => {
  beforeEach(() => {
    useTerminalStore.setState({
      sessions: [
        session("a", wt),
        session("b", wt),
        session("c", wt),
        session("x", "/other"),
      ],
      activeByWorktree: { [wt]: "b" },
    });
  });

  it("moves to the previous tab when the active one closes", async () => {
    await useTerminalStore.getState().close("b");

    const state = useTerminalStore.getState();
    expect(state.sessions.map((s) => s.id)).toEqual(["a", "c", "x"]);
    expect(state.activeByWorktree[wt]).toBe("a");
    expect(api.closeTerminal).toHaveBeenCalledWith("b");
  });

  it("moves to the next tab when the first one closes", async () => {
    useTerminalStore.setState({ activeByWorktree: { [wt]: "a" } });

    await useTerminalStore.getState().close("a");

    expect(useTerminalStore.getState().activeByWorktree[wt]).toBe("b");
  });

  it("keeps the active tab when another one closes", async () => {
    await useTerminalStore.getState().close("c");

    expect(useTerminalStore.getState().activeByWorktree[wt]).toBe("b");
  });

  it("forgets the active tab when the last one closes", async () => {
    await useTerminalStore.getState().close("x");

    expect(useTerminalStore.getState().sessions.map((s) => s.id)).toEqual([
      "a",
      "b",
      "c",
    ]);
  });

  it("ignores an unknown id", async () => {
    await useTerminalStore.getState().close("nope");
    expect(api.closeTerminal).not.toHaveBeenCalled();
  });

  it("closes every session of a worktree and leaves the others", async () => {
    await useTerminalStore.getState().closeAllIn(wt);

    const state = useTerminalStore.getState();
    expect(state.sessions.map((s) => s.id)).toEqual(["x"]);
    expect(state.activeByWorktree[wt]).toBeUndefined();
    expect(api.closeTerminal).toHaveBeenCalledTimes(3);
  });
});

describe("markExited", () => {
  it("flags only that session and keeps its exit code", () => {
    useTerminalStore.setState({
      sessions: [session("a", wt), session("b", wt)],
    });

    useTerminalStore.getState().markExited("a", 7);

    const [a, b] = useTerminalStore.getState().sessions;
    expect(a).toMatchObject({ running: false, exitCode: 7 });
    expect(b).toMatchObject({ running: true, exitCode: null });
  });
});

describe("activeSessionId", () => {
  const sessions = [session("a", wt), session("b", wt), session("x", "/other")];

  it("uses the remembered tab when it still exists", () => {
    expect(activeSessionId(sessions, { [wt]: "a" }, wt)).toBe("a");
  });

  it("falls back to the newest tab of that worktree", () => {
    expect(activeSessionId(sessions, { [wt]: "gone" }, wt)).toBe("b");
    expect(activeSessionId(sessions, {}, wt)).toBe("b");
  });

  it("is null without sessions or without a worktree", () => {
    expect(activeSessionId(sessions, {}, "/empty")).toBeNull();
    expect(activeSessionId(sessions, {}, null)).toBeNull();
  });
});
