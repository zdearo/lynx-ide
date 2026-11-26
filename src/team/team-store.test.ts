import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { toast } from "sonner";

import type { TeamSnapshot, TerminalInfo } from "@/bindings";
import { IpcRequestError } from "@/lib/ipc-result";
import type { TeamStatus, WorkerState } from "@/team/team-fixtures";
import { onTeamChanged, teamBackend } from "@/team/team-backend";
import {
  agentNameOf,
  membersOfWorktree,
  statusOfSession,
  statusOfWorker,
  useTeamStore,
  watchTeam,
  OPEN_PER_FRAME,
} from "@/team/team-store";
import { agentLine, requestCloseTerminal } from "@/terminal/terminal-actions";
import { useTerminalStore } from "@/terminal/terminal-store";
import { useWorkspaceStore } from "@/workspace/workspace-store";
import { copy } from "@/ui-copy";

vi.mock("sonner", () => ({ toast: { error: vi.fn() } }));
// The mock backend's live fields are not under test here: the store runs as the real app does.
vi.mock("@/dev/mock-mode", () => ({ isMockBackend: () => false }));
vi.mock("@/team/team-backend", () => ({
  teamBackend: {
    snapshot: vi.fn(),
    addMember: vi.fn(),
    setParent: vi.fn(),
    setOrchestrator: vi.fn(),
    setSession: vi.fn(),
    dismissMember: vi.fn(),
    setLinked: vi.fn(),
    setExpanded: vi.fn(),
  },
  onTeamChanged: vi.fn(),
}));
// The terminal is the session store's: a fake that records what the team asks of it.
const terminal = vi.hoisted(() => ({
  create: vi.fn(),
  close: vi.fn(),
  sessions: [] as TerminalInfo[],
}));
// The sessions the backend closed on its own, which the team keeps the cards of.
const closedByBackend = vi.hoisted(() => new Set<string>());
vi.mock("@/terminal/terminal-store", () => ({
  useTerminalStore: { getState: () => terminal, subscribe: vi.fn() },
  isClosedByBackend: (id: string) => closedByBackend.has(id),
}));
vi.mock("@/terminal/terminal-actions", () => ({
  agentLine: vi.fn(),
  requestCloseTerminal: vi.fn(),
}));

const api = vi.mocked(teamBackend);
const initialState = useTeamStore.getState();

const AUTH = "/work/auth";
const DOCS = "/work/docs";

const snapshot = (overrides: Partial<TeamSnapshot> = {}): TeamSnapshot => ({
  members: [
    {
      id: "lead",
      worktree: AUTH,
      kind: "agent",
      role: "Lead",
      harness: "claude",
      botId: null,
      parentId: null,
      orchestrator: true,
      task: "",
      acceptance: "",
      taskState: null,
      dependsOn: [],
      executionWorktree: null,
      dispatchState: null,
      sessionId: null,
      workerState: null,
      expanded: false,
      lastMessage: null,
    },
    {
      id: "builder",
      worktree: AUTH,
      kind: "agent",
      role: "Builder",
      harness: "codex",
      botId: null,
      parentId: "lead",
      orchestrator: true,
      task: "login form",
      acceptance: "The form submits.",
      taskState: null,
      dependsOn: [],
      executionWorktree: null,
      dispatchState: null,
      sessionId: null,
      workerState: null,
      expanded: false,
      lastMessage: null,
    },
    {
      id: "reviewer",
      worktree: AUTH,
      kind: "agent",
      role: "Reviewer",
      harness: "claude",
      botId: null,
      parentId: null,
      orchestrator: false,
      task: "",
      acceptance: "",
      taskState: null,
      dependsOn: [],
      executionWorktree: null,
      dispatchState: null,
      sessionId: null,
      workerState: null,
      expanded: false,
      lastMessage: null,
    },
    {
      id: "docs-lead",
      worktree: DOCS,
      kind: "bot",
      role: "Lead",
      harness: null,
      botId: "lead",
      parentId: null,
      orchestrator: true,
      task: "",
      acceptance: "",
      taskState: null,
      dependsOn: [],
      executionWorktree: null,
      dispatchState: null,
      sessionId: null,
      workerState: null,
      expanded: false,
      lastMessage: null,
    },
  ],
  pairs: [{ a: "builder", b: "reviewer" }],
  notes: [],
  ...overrides,
});

/** A terminal session as the store lists it: a Claude agent in the selected worktree, running. */
const session = (
  id: string,
  overrides: Partial<TerminalInfo> = {},
): TerminalInfo => ({
  id,
  worktreePath: AUTH,
  command: "claude",
  shell: "zsh",
  running: true,
  hibernated: false,
  exitCode: null,
  startedAt: 0,
  harness: "claude",
  agentStatus: "idle",
  restorable: false,
  title: null,
  ...overrides,
});

const store = () => useTeamStore.getState();
const member = (id: string) => store().members.find((m) => m.id === id);

/** Loads the team the backend would send, as the Team view does on open. */
async function loadTeam(team: TeamSnapshot = snapshot()) {
  api.snapshot.mockResolvedValue(team);
  await store().load();
}

const refused = (message: string) =>
  new IpcRequestError({ kind: "teamRefused", message });

/** The default team with the given members' branches open, as the backend flags them. */
const withExpanded = (ids: string[]): TeamSnapshot => {
  const base = snapshot();
  return {
    ...base,
    members: base.members.map((member) => ({
      ...member,
      expanded: ids.includes(member.id),
    })),
  };
};

beforeEach(() => {
  vi.resetAllMocks();
  terminal.sessions = [];
  closedByBackend.clear();
  useTeamStore.setState(initialState, true);
  useWorkspaceStore.setState({ selectedWorktreePath: null });
});

// Frames the store asks for are kept here until the test runs them: `step` runs the pending ones, `runAll` until none is left.
function stubFrames() {
  const callbacks = new Map<number, FrameRequestCallback>();
  let nextId = 1;
  vi.stubGlobal("requestAnimationFrame", (callback: FrameRequestCallback) => {
    const id = nextId++;
    callbacks.set(id, callback);
    return id;
  });
  vi.stubGlobal("cancelAnimationFrame", (id: number) => {
    callbacks.delete(id);
  });
  const step = () => {
    const pending = [...callbacks.values()];
    callbacks.clear();
    pending.forEach((callback) => callback(0));
    return pending.length;
  };
  return {
    step,
    runAll: () => {
      while (step() > 0) {
        // keep running the frames the batches ask for
      }
    },
  };
}

afterEach(() => {
  vi.unstubAllGlobals();
});

describe("team load", () => {
  it("shows the members and lines the backend sends, each in its worktree", async () => {
    await loadTeam();

    expect(store()).toMatchObject({
      loaded: true,
      pairs: [{ from: "builder", to: "reviewer" }],
    });
    expect(member("lead")).toMatchObject({ worktree: AUTH });
    expect(member("builder")).toMatchObject({
      kind: "agent",
      harness: "codex",
      parentId: "lead",
      status: "idle",
      output: [],
    });
    expect(member("docs-lead")).toMatchObject({ kind: "bot", botId: "lead" });
  });

  it("gives the members of one worktree, and none with no worktree selected", async () => {
    await loadTeam();

    const ids = (worktree: string | null) =>
      membersOfWorktree(store().members, worktree).map((m) => m.id);
    expect(ids(AUTH)).toEqual(["lead", "builder", "reviewer"]);
    expect(ids(DOCS)).toEqual(["docs-lead"]);
    expect(ids(null)).toEqual([]);
  });

  it("takes a member's activity from its newest message, with the message's kind", async () => {
    const team = snapshot();
    team.members[0] = {
      ...team.members[0],
      lastMessage: {
        kind: "needsYou",
        body: "pick a branch",
        createdAt: 1_700,
      },
    };
    await loadTeam(team);

    expect(member("lead")?.activity).toEqual({
      text: "pick a branch",
      at: 1_700,
      kind: "needsYou",
    });
    expect(member("builder")?.activity).toEqual({ text: "", at: 0 });
  });

  it("keeps the team's notes from the snapshot", async () => {
    const notes = [{ worktree: AUTH, name: "plan", body: "step 1" }];
    await loadTeam(snapshot({ notes }));

    expect(store().notes).toEqual(notes);
  });

  it("drops a read that a newer read overtakes", async () => {
    let finishOlder: (team: TeamSnapshot) => void = () => {};
    api.snapshot
      .mockReturnValueOnce(
        new Promise<TeamSnapshot>((resolve) => {
          finishOlder = resolve;
        }),
      )
      .mockResolvedValueOnce(snapshot({ members: [], pairs: [] }));

    const older = store().load();
    await store().load();
    finishOlder(snapshot());
    await older;

    expect(store().members).toEqual([]);
  });

  it("shows a message and keeps the screen when the backend cannot send the team", async () => {
    api.snapshot.mockRejectedValue(new Error("the database is locked"));

    await store().load();

    expect(store().loaded).toBe(false);
    expect(toast.error).toHaveBeenCalledWith(copy.team.loadFailed, {
      description: "the database is locked",
    });
  });

  it("refuses a bot the backend sends with no bot id, rather than guessing one", async () => {
    await loadTeam(
      snapshot({
        members: [
          { ...snapshot().members[0], kind: "bot", harness: null, botId: null },
        ],
      }),
    );

    expect(store().loaded).toBe(false);
    expect(toast.error).toHaveBeenCalled();
  });

  it("keeps an agent the orchestrator added with no harness yet: it has not started a worker", async () => {
    await loadTeam(
      snapshot({
        members: [
          {
            ...snapshot().members[0],
            harness: null,
            taskState: "pending",
            dependsOn: [],
          },
        ],
      }),
    );

    expect(store().loaded).toBe(true);
    expect(store().members[0]).toMatchObject({
      kind: "agent",
      harness: null,
      taskState: "pending",
    });
  });

  it("reads the team again when the backend says it changed, until the view closes", async () => {
    let handler: () => void = () => {};
    const stop = vi.fn();
    vi.mocked(onTeamChanged).mockImplementation((next) => {
      handler = next;
      return stop;
    });
    api.snapshot.mockResolvedValue(snapshot());

    expect(watchTeam()).toBe(stop);
    await vi.waitFor(() => expect(api.snapshot).toHaveBeenCalledTimes(1));

    handler();
    await vi.waitFor(() => expect(api.snapshot).toHaveBeenCalledTimes(2));
  });

  it("keeps the member objects and the list itself when a read changes nothing", async () => {
    await loadTeam();
    const before = store().members;
    const builder = member("builder");

    await loadTeam();

    expect(store().members).toBe(before);
    expect(member("builder")).toBe(builder);
  });

  it("gives a new object only to the member whose content changed", async () => {
    await loadTeam();
    const before = store().members;
    const team = snapshot();
    team.members[1] = { ...team.members[1], task: "signup form" };

    await loadTeam(team);

    expect(member("builder")).toMatchObject({ task: "signup form" });
    expect(member("builder")).not.toBe(before[1]);
    expect(member("lead")).toBe(before[0]);
  });
});

describe("team layout", () => {
  it("keeps the extra rows at zero or more", () => {
    store().setExtraRows("builder", 2);
    expect(store().extraRows.builder).toBe(2);
    store().setExtraRows("builder", -4);
    expect(store().extraRows.builder).toBe(0);
  });
});

describe("team changes", () => {
  beforeEach(async () => {
    await loadTeam();
    useWorkspaceStore.setState({ selectedWorktreePath: AUTH });
    vi.mocked(agentLine).mockResolvedValue("claude");
    terminal.create.mockResolvedValue({ id: "s1" });
  });

  it("shows the backend's message for a refused change, and changes nothing", async () => {
    api.setParent.mockRejectedValue(
      refused("an orchestrator cannot answer to one of its own members"),
    );

    await store().setParent("lead", "builder");

    expect(api.setParent).toHaveBeenCalledWith("lead", "builder");
    expect(toast.error).toHaveBeenCalledWith(copy.team.changeRefused, {
      description: "an orchestrator cannot answer to one of its own members",
    });
    expect(store().expanded).not.toContain("builder");
    expect(member("lead")?.parentId).toBeNull();
  });

  it("opens a shut new orchestrator through the backend once it accepts the member", async () => {
    api.setParent.mockResolvedValue(null);
    api.setExpanded.mockResolvedValue(null);

    await store().setParent("reviewer", "builder");

    expect(api.setExpanded).toHaveBeenCalledWith("builder", true);
    expect(store().extraRows.reviewer).toBe(0);
  });

  it("does not ask for a new orchestrator that is already open", async () => {
    await loadTeam(withExpanded(["builder"]));
    api.setParent.mockResolvedValue(null);

    await store().setParent("reviewer", "builder");

    expect(api.setExpanded).not.toHaveBeenCalled();
  });

  it("forgets a dismissed member that was picked", async () => {
    api.dismissMember.mockResolvedValue(null);
    store().select("builder");

    await store().dismiss("builder");

    expect(api.dismissMember).toHaveBeenCalledWith("builder");
    expect(store().selectedId).toBeNull();
  });

  it("sends a line change with both ends and the flag", async () => {
    api.setLinked.mockResolvedValue(null);

    await store().setLinked("reviewer", "lead", true);

    expect(api.setLinked).toHaveBeenCalledWith("reviewer", "lead", true);
  });

  it("turns an orchestrator off and on through the backend", async () => {
    api.setOrchestrator.mockResolvedValue(null);

    await store().setOrchestrator("builder", false);

    expect(api.setOrchestrator).toHaveBeenCalledWith("builder", false);
  });

  it("adds a member, picks it and opens its parent", async () => {
    api.addMember.mockResolvedValue({
      ...snapshot().members[2],
      id: "scout",
      role: "Scout",
      parentId: "lead",
    });
    store().setAddOpen(true);

    const added = await store().addMember({
      kind: "agent",
      role: "Scout",
      harness: "claude",
      parentId: "lead",
      orchestrator: false,
    });

    expect(added).toBe(true);
    // No task is sent: the orchestrator gives it later.
    expect(api.addMember).toHaveBeenCalledWith(
      expect.objectContaining({
        role: "Scout",
        parentId: "lead",
        task: "",
        worktree: AUTH,
      }),
    );
    expect(store()).toMatchObject({ selectedId: "scout", addOpen: false });
    expect(api.setExpanded).toHaveBeenCalledWith("lead", true);
  });

  it("keeps the dialog open when the backend refuses a member", async () => {
    api.addMember.mockRejectedValue(
      refused("only an orchestrator can have members under it"),
    );

    const added = await store().addMember({
      kind: "agent",
      role: "Scout",
      harness: "claude",
      parentId: "reviewer",
      orchestrator: false,
    });

    expect(added).toBe(false);
    expect(toast.error).toHaveBeenCalledWith(copy.team.changeRefused, {
      description: "only an orchestrator can have members under it",
    });
    expect(store().selectedId).toBeNull();
  });
});

describe("team expansion", () => {
  it("opens the members the backend flags as expanded, and no others", async () => {
    await loadTeam(withExpanded(["lead"]));

    expect(store().expanded).toEqual(["lead"]);
  });

  it("asks the backend to open a shut member, and shows nothing until the team is read again", async () => {
    await loadTeam();
    api.setExpanded.mockResolvedValue(null);

    await store().toggle("builder");

    expect(api.setExpanded).toHaveBeenCalledWith("builder", true);
    expect(store().expanded).toEqual([]);
  });

  it("asks the backend to shut an open member, and keeps it open until the team is read", async () => {
    await loadTeam(withExpanded(["lead"]));
    api.setExpanded.mockResolvedValue(null);

    await store().toggle("lead");

    expect(api.setExpanded).toHaveBeenCalledWith("lead", false);
    expect(store().expanded).toEqual(["lead"]);
  });

  it("shows the flag the backend sends back, once the team is read again", async () => {
    await loadTeam();
    api.setExpanded.mockResolvedValue(null);
    await store().toggle("builder");

    await loadTeam(withExpanded(["builder"]));

    expect(store().expanded).toEqual(["builder"]);
  });

  it("shows the backend's message for a refused toggle, and leaves the member as it was", async () => {
    await loadTeam();
    api.setExpanded.mockRejectedValue(refused("no member with id builder"));

    await store().toggle("builder");

    expect(toast.error).toHaveBeenCalledWith(copy.team.changeRefused, {
      description: "no member with id builder",
    });
    expect(store().expanded).toEqual([]);
  });

  it("clears the pick when collapsing its parent hides it", async () => {
    await loadTeam(withExpanded(["lead"]));
    store().select("builder");
    api.setExpanded.mockResolvedValue(null);

    await store().toggle("lead");

    expect(api.setExpanded).toHaveBeenCalledWith("lead", false);
    expect(store().selectedId).toBeNull();
  });

  it("keeps the pick when a collapse hides nothing picked, or the backend refuses it", async () => {
    await loadTeam(withExpanded(["lead", "builder"]));
    store().select("reviewer");
    api.setExpanded.mockResolvedValue(null);
    await store().toggle("builder");
    expect(store().selectedId).toBe("reviewer");

    store().select("builder");
    api.setExpanded.mockRejectedValue(refused("no member with id lead"));
    await store().toggle("lead");
    expect(store().selectedId).toBe("builder");
  });
});

describe("team sessions", () => {
  // The listener the store subscribed to the terminal store when it loaded.
  const onTerminalChange = vi.mocked(useTerminalStore.subscribe).mock
    .calls[0]?.[0] as (state: unknown, previous: unknown) => void;
  const agentDraft = {
    kind: "agent" as const,
    role: "Scout",
    harness: "claude" as const,
    parentId: "lead",
    orchestrator: false,
    task: "check it",
    acceptance: "",
  };

  /** The team the backend sends, with the given agents running in sessions in the selected worktree. */
  function teamWithSessions(
    sessions: Record<string, string>,
    members = snapshot().members,
  ): TeamSnapshot {
    return {
      ...snapshot(),
      members: members.map((member) =>
        member.id in sessions
          ? {
              ...member,
              worktree: AUTH,
              sessionId: sessions[member.id],
            }
          : member,
      ),
    };
  }

  beforeEach(async () => {
    await loadTeam();
    useWorkspaceStore.setState({ selectedWorktreePath: AUTH });
    vi.mocked(agentLine).mockResolvedValue("claude");
    terminal.create.mockResolvedValue({ id: "s1" });
  });

  it("starts the agent in the selected worktree and adds it with that session", async () => {
    api.addMember.mockResolvedValue({
      ...snapshot().members[2],
      id: "scout",
      role: "Scout",
      worktree: AUTH,
      sessionId: "s1",
    });

    expect(await store().addMember(agentDraft)).toBe(true);

    expect(agentLine).toHaveBeenCalledWith("claude");
    expect(terminal.create).toHaveBeenCalledWith(AUTH, "claude");
    expect(api.addMember).toHaveBeenCalledWith(
      expect.objectContaining({
        role: "Scout",
        worktree: AUTH,
        sessionId: "s1",
      }),
    );
    expect(store().addingSession).toBe(false);
  });

  it("marks the add as starting a session only while the terminal is created", async () => {
    let flagDuringCreate = false;
    terminal.create.mockImplementation(async () => {
      flagDuringCreate = store().addingSession;
      return { id: "s1" };
    });
    api.addMember.mockResolvedValue(snapshot().members[2]);

    await store().addMember(agentDraft);

    expect(flagDuringCreate).toBe(true);
    expect(store().addingSession).toBe(false);
  });

  it("refuses an agent when no worktree is selected, and changes nothing", async () => {
    useWorkspaceStore.setState({ selectedWorktreePath: null });

    expect(await store().addMember(agentDraft)).toBe(false);

    expect(toast.error).toHaveBeenCalledWith(copy.team.add.noWorktree);
    expect(terminal.create).not.toHaveBeenCalled();
    expect(api.addMember).not.toHaveBeenCalled();
    expect(store().addOpen).toBe(false);
  });

  it("closes the session it started when the team refuses the member", async () => {
    api.addMember.mockRejectedValue(
      refused("an orchestrator cannot answer to one of its own members"),
    );
    store().setAddOpen(true);

    expect(await store().addMember(agentDraft)).toBe(false);

    expect(terminal.close).toHaveBeenCalledWith("s1");
    expect(toast.error).toHaveBeenCalledWith(copy.team.changeRefused, {
      description: "an orchestrator cannot answer to one of its own members",
    });
    expect(store().addOpen).toBe(true);
  });

  it("adds a bot to the selected worktree's team, with no session, and starts no terminal", async () => {
    api.addMember.mockResolvedValue(snapshot().members[3]);

    expect(
      await store().addMember({ kind: "bot", botId: "lead", role: "Lead" }),
    ).toBe(true);

    expect(api.addMember).toHaveBeenCalledWith({
      kind: "bot",
      botId: "lead",
      role: "Lead",
      worktree: AUTH,
    });
    expect(terminal.create).not.toHaveBeenCalled();
  });

  it("returns the session of a dismissed agent to the close policy", async () => {
    await loadTeam(teamWithSessions({ builder: "s1" }));
    api.dismissMember.mockResolvedValue("s1");

    await store().dismiss("builder");
    // The backend sends the team again once the member is gone.
    await loadTeam(
      teamWithSessions(
        {},
        snapshot().members.filter((member) => member.id !== "builder"),
      ),
    );

    expect(api.dismissMember).toHaveBeenCalledWith("builder");
    expect(requestCloseTerminal).toHaveBeenCalledWith("s1");
    expect(terminal.close).not.toHaveBeenCalled();
  });

  it("asks nothing about a terminal when a bot is dismissed", async () => {
    api.dismissMember.mockResolvedValue(null);

    await store().dismiss("docs-lead");

    expect(requestCloseTerminal).not.toHaveBeenCalled();
  });

  it("closes nothing on the first read, even with agents in sessions", async () => {
    // Back to a store that has not read the team yet: the first read has no team before it.
    useTeamStore.setState({ loaded: false, members: [] });

    await loadTeam(teamWithSessions({ reviewer: "s2", lead: "s3" }));

    expect(terminal.close).not.toHaveBeenCalled();
  });

  it("closes the session of an agent that left the team, and keeps the ones still in it", async () => {
    await loadTeam(teamWithSessions({ reviewer: "s2", lead: "s3" }));

    await loadTeam(
      teamWithSessions(
        { lead: "s3" },
        snapshot().members.filter((member) => member.id !== "reviewer"),
      ),
    );

    expect(terminal.close).toHaveBeenCalledTimes(1);
    expect(terminal.close).toHaveBeenCalledWith("s2");
  });

  it("does not close a dismissed agent's session when the team read drops it first", async () => {
    await loadTeam(teamWithSessions({ builder: "s1" }));
    // The backend sends the team again before the dismiss has answered.
    api.dismissMember.mockImplementation(async () => {
      await loadTeam(
        teamWithSessions(
          {},
          snapshot().members.filter((member) => member.id !== "builder"),
        ),
      );
      return "s1";
    });

    await store().dismiss("builder");

    expect(terminal.close).not.toHaveBeenCalled();
    expect(requestCloseTerminal).toHaveBeenCalledWith("s1");
  });

  it("dismisses the agent whose terminal was closed, and leaves the others", async () => {
    await loadTeam(teamWithSessions({ builder: "s1", reviewer: "s2" }));
    api.dismissMember.mockResolvedValue(null);

    onTerminalChange(
      { sessions: [{ id: "s2" }] } as never,
      { sessions: [{ id: "s1" }, { id: "s2" }] } as never,
    );

    expect(api.dismissMember).toHaveBeenCalledTimes(1);
    expect(api.dismissMember).toHaveBeenCalledWith("builder");
  });

  it("dismisses the agent whose terminal was closed though the team view never opened", async () => {
    // The app starts watching the team, so a session closed before the team view opens still leaves it.
    // A team read without the builder forgets its earlier dismissal, as the app does when a member leaves.
    await loadTeam(
      teamWithSessions(
        {},
        snapshot().members.filter((m) => m.id !== "builder"),
      ),
    );
    useTeamStore.setState({ members: [] });
    api.snapshot.mockResolvedValue(teamWithSessions({ builder: "s1" }));
    api.dismissMember.mockResolvedValue(null);
    watchTeam();
    await vi.waitFor(() => expect(store().members.length).toBeGreaterThan(0));

    onTerminalChange(
      { sessions: [] } as never,
      { sessions: [{ id: "s1" }] } as never,
    );

    expect(api.dismissMember).toHaveBeenCalledWith("builder");
  });

  it("does not dismiss again an agent the team already dismissed", async () => {
    await loadTeam(teamWithSessions({ builder: "s1" }));
    api.dismissMember.mockResolvedValue("s1");

    await store().dismiss("builder");
    onTerminalChange(
      { sessions: [] } as never,
      { sessions: [{ id: "s1" }] } as never,
    );

    expect(api.dismissMember).toHaveBeenCalledTimes(1);
  });

  it("ignores a closed session that no agent of the team owns", async () => {
    await loadTeam(teamWithSessions({ builder: "s1" }));

    onTerminalChange(
      { sessions: [] } as never,
      { sessions: [{ id: "other" }] } as never,
    );

    expect(api.dismissMember).not.toHaveBeenCalled();
  });

  it("watches a new member that takes the id of a dismissed one", async () => {
    await loadTeam(teamWithSessions({ builder: "s1" }));
    api.dismissMember.mockResolvedValue(null);
    await store().dismiss("builder");
    // The team reads again without the Builder; a new Builder comes with the same id and its own session.
    await loadTeam(
      teamWithSessions(
        {},
        snapshot().members.filter((m) => m.id !== "builder"),
      ),
    );
    await loadTeam(teamWithSessions({ builder: "s9" }));
    api.dismissMember.mockClear();

    onTerminalChange(
      { sessions: [] } as never,
      { sessions: [{ id: "s9" }] } as never,
    );

    expect(api.dismissMember).toHaveBeenCalledWith("builder");
  });

  it.each<[string, TerminalInfo | undefined, TeamStatus]>([
    ["working", session("s", { agentStatus: "working" }), "working"],
    ["needs you", session("s", { agentStatus: "needsYou" }), "needsYou"],
    ["done", session("s", { agentStatus: "done" }), "done"],
    ["idle when the agent reports nothing", session("s"), "idle"],
    [
      "closed once the program has ended",
      session("s", { running: false, exitCode: 0 }),
      "closed",
    ],
    ["closed when the session is gone", undefined, "closed"],
  ])("maps a session to a card's status: %s", (_name, input, status) => {
    expect(statusOfSession(input)).toBe(status);
  });

  it.each<[WorkerState, TeamStatus]>([
    ["running", "working"],
    ["done", "done"],
    ["failed", "failed"],
    ["stopped", "closed"],
  ])("maps a worker in state %s to a card's status %s", (state, status) => {
    expect(statusOfWorker(state)).toBe(status);
  });

  // A team whose builder is a worker in the given state. It keeps its session, as a worker does.
  function workerTeam(workerState: string | null): TeamSnapshot {
    const team = teamWithSessions({ builder: "s1" });
    return {
      ...team,
      members: team.members.map((member) =>
        member.id === "builder" ? { ...member, workerState } : member,
      ),
    };
  }

  it("shows a worker's card from its worker state, not from its session", async () => {
    terminal.sessions = [session("s1", { agentStatus: "needsYou" })];
    await loadTeam(workerTeam("failed"));

    expect(member("builder")?.status).toBe("failed");
  });

  it("reads the team again when a worker's process ends, so its card follows the backend", async () => {
    terminal.sessions = [session("s1")];
    await loadTeam(workerTeam("running"));
    api.snapshot.mockClear();

    onTerminalChange(
      { sessions: [session("s1", { running: false })] } as never,
      { sessions: [session("s1")] } as never,
    );

    expect(api.snapshot).toHaveBeenCalledTimes(1);
  });

  it("does not read the team again for an agent that is not a worker", async () => {
    terminal.sessions = [session("s1")];
    await loadTeam(teamWithSessions({ builder: "s1" }));
    api.snapshot.mockClear();

    onTerminalChange(
      { sessions: [session("s1", { running: false })] } as never,
      { sessions: [session("s1")] } as never,
    );

    expect(api.snapshot).not.toHaveBeenCalled();
  });

  it("keeps a worker's card when the backend closed its terminal, so it shows how the worker ended", async () => {
    await loadTeam(workerTeam("done"));
    closedByBackend.add("s1");

    onTerminalChange(
      { sessions: [] } as never,
      { sessions: [{ id: "s1" }] } as never,
    );

    expect(api.dismissMember).not.toHaveBeenCalled();
    expect(member("builder")?.status).toBe("done");
  });

  it("closes the terminal when the member it shows leaves the team", async () => {
    await loadTeam(teamWithSessions({ builder: "s1" }));
    store().openTerminal("builder");

    await loadTeam(
      snapshot({
        members: snapshot().members.filter((m) => m.id !== "builder"),
      }),
    );

    expect(store().openIds).toEqual([]);
  });

  it("clears the pick when the worktree changes, and keeps each member's open terminal", async () => {
    await loadTeam();
    store().select("builder");
    store().openTerminal("builder");

    useWorkspaceStore.setState({ selectedWorktreePath: DOCS });

    expect(store()).toMatchObject({ selectedId: null, openIds: ["builder"] });
  });

  it("shows an agent's status from its session, and follows the terminal as it changes", async () => {
    terminal.sessions = [session("s1", { agentStatus: "working" })];
    await loadTeam(teamWithSessions({ builder: "s1" }));
    expect(member("builder")?.status).toBe("working");

    onTerminalChange(
      { sessions: [session("s1", { agentStatus: "needsYou" })] } as never,
      { sessions: terminal.sessions } as never,
    );
    expect(member("builder")?.status).toBe("needsYou");

    onTerminalChange(
      { sessions: [session("s1", { running: false })] } as never,
      { sessions: [session("s1", { agentStatus: "needsYou" })] } as never,
    );
    expect(member("builder")?.status).toBe("closed");
  });

  it("shows closed for an agent whose session is gone", async () => {
    terminal.sessions = [];
    await loadTeam(teamWithSessions({ builder: "s9" }));

    expect(member("builder")?.status).toBe("closed");
  });

  it("shows idle for an agent with no session at all", async () => {
    await loadTeam();

    expect(member("builder")).toMatchObject({
      sessionId: null,
      status: "idle",
    });
  });

  it("adds a terminal agent to the team of the worktree it runs in, as a loose member", () => {
    api.addMember.mockResolvedValue(snapshot().members[2]);

    onTerminalChange(
      { sessions: [session("a1")] } as never,
      { sessions: [] } as never,
    );

    expect(api.addMember).toHaveBeenCalledTimes(1);
    expect(api.addMember).toHaveBeenCalledWith({
      kind: "agent",
      role: agentNameOf("claude"),
      harness: "claude",
      parentId: null,
      orchestrator: false,
      dependsOn: [],
      task: "",
      acceptance: "",
      worktree: AUTH,
      sessionId: "a1",
    });
  });

  it("adopts an agent into its own worktree's team, whichever worktree is selected", () => {
    api.addMember.mockResolvedValue(snapshot().members[2]);
    useWorkspaceStore.setState({ selectedWorktreePath: null });

    onTerminalChange(
      { sessions: [session("a8", { worktreePath: DOCS })] } as never,
      { sessions: [] } as never,
    );

    expect(api.addMember).toHaveBeenCalledWith(
      expect.objectContaining({ worktree: DOCS, sessionId: "a8" }),
    );
  });

  it("does not add a plain shell", () => {
    onTerminalChange(
      { sessions: [session("sh1", { harness: null, command: null })] } as never,
      { sessions: [] } as never,
    );

    expect(api.addMember).not.toHaveBeenCalled();
  });

  it("does not add a session a member already runs in, even when a restart reloads it", async () => {
    await loadTeam(teamWithSessions({ builder: "a2" }));

    onTerminalChange(
      { sessions: [session("a2", { harness: "codex" })] } as never,
      { sessions: [] } as never,
    );

    expect(api.addMember).not.toHaveBeenCalled();
  });

  it("leaves the session the team's own add is starting to that add", async () => {
    terminal.create.mockImplementation(async (worktree: string) => {
      const started = session("t1", { worktreePath: worktree });
      // The terminal store takes the session while the team starts it.
      onTerminalChange(
        { sessions: [started] } as never,
        { sessions: [] } as never,
      );
      return started;
    });
    api.addMember.mockResolvedValue({
      ...snapshot().members[2],
      id: "scout",
      sessionId: "t1",
    });

    expect(await store().addMember(agentDraft)).toBe(true);

    expect(api.addMember).toHaveBeenCalledTimes(1);
    expect(api.addMember).toHaveBeenCalledWith(
      expect.objectContaining({ sessionId: "t1" }),
    );
  });

  it("adds nothing while the team is adding a session", () => {
    useTeamStore.setState({ addingSession: true });

    onTerminalChange(
      { sessions: [session("a6")] } as never,
      { sessions: [] } as never,
    );

    expect(api.addMember).not.toHaveBeenCalled();
  });

  it("adds nothing before the team is read, and adds the session once a team read brings it", async () => {
    useTeamStore.setState({ loaded: false });
    api.addMember.mockResolvedValue(snapshot().members[2]);

    onTerminalChange(
      { sessions: [session("a7")] } as never,
      { sessions: [] } as never,
    );
    expect(api.addMember).not.toHaveBeenCalled();

    terminal.sessions = [session("a7")];
    await loadTeam();

    expect(api.addMember).toHaveBeenCalledTimes(1);
    expect(api.addMember).toHaveBeenCalledWith(
      expect.objectContaining({ sessionId: "a7" }),
    );
  });

  it("adds a session once even when the store reports it again", async () => {
    api.addMember.mockResolvedValue(snapshot().members[2]);
    const reported = { sessions: [session("a3")] } as never;

    onTerminalChange(reported, { sessions: [] } as never);
    // The team reads before its member is in the list, so only the taken set keeps the session out.
    await loadTeam();
    onTerminalChange(reported, { sessions: [] } as never);

    expect(api.addMember).toHaveBeenCalledTimes(1);
  });

  it("dismisses an adopted member once, when its session closes", async () => {
    const adopted = {
      ...snapshot().members[2],
      id: "scout",
      worktree: AUTH,
      sessionId: "a4",
    };
    terminal.sessions = [session("a4")];
    api.addMember.mockResolvedValue(adopted);
    api.dismissMember.mockResolvedValue("a4");
    onTerminalChange(
      { sessions: [session("a4")] } as never,
      { sessions: [] } as never,
    );
    await vi.waitFor(() => expect(api.addMember).toHaveBeenCalledTimes(1));
    await loadTeam({
      ...snapshot(),
      members: [...snapshot().members, adopted],
    });

    terminal.sessions = [];
    onTerminalChange(
      { sessions: [] } as never,
      { sessions: [session("a4")] } as never,
    );
    onTerminalChange(
      { sessions: [] } as never,
      { sessions: [session("a4")] } as never,
    );

    expect(api.dismissMember).toHaveBeenCalledTimes(1);
    expect(api.dismissMember).toHaveBeenCalledWith("scout");
  });

  it("dismisses an adopted member whose session closed while the team added it", async () => {
    // The session is gone from the store by the time the team answers.
    terminal.sessions = [];
    api.addMember.mockResolvedValue({
      ...snapshot().members[2],
      id: "scout",
      sessionId: "a5",
    });
    api.dismissMember.mockResolvedValue("a5");

    onTerminalChange(
      { sessions: [session("a5")] } as never,
      { sessions: [] } as never,
    );

    await vi.waitFor(() =>
      expect(api.dismissMember).toHaveBeenCalledWith("scout"),
    );
  });

  describe("reviving an agent whose session ended", () => {
    /** The reviewer, a Claude agent, is on its old session, which is not open: its terminal is closed. */
    const closedReviewer = () =>
      loadTeam(teamWithSessions({ reviewer: "old-1" }));

    it("starts a fresh agent in the same worktree and ties the new session to the member", async () => {
      await closedReviewer();
      vi.mocked(agentLine).mockResolvedValue("claude");
      terminal.create.mockResolvedValue(session("s2"));
      api.setSession.mockResolvedValue(null);
      api.snapshot.mockResolvedValue(teamWithSessions({ reviewer: "s2" }));

      expect(await store().reviveMember("reviewer")).toBe(true);

      expect(agentLine).toHaveBeenCalledWith("claude");
      expect(terminal.create).toHaveBeenCalledWith(AUTH, "claude");
      expect(api.setSession).toHaveBeenCalledWith("reviewer", "s2");
      expect(member("reviewer")).toMatchObject({ sessionId: "s2" });
      expect(api.addMember).not.toHaveBeenCalled();
    });

    it("does not adopt the new session as a second loose member, even when the store reports it while it is tied", async () => {
      await closedReviewer();
      vi.mocked(agentLine).mockResolvedValue("claude");
      const started = session("s2");
      terminal.create.mockImplementation(async () => {
        // The store takes the session while the team starts it: nothing is adopted meanwhile.
        terminal.sessions = [started];
        onTerminalChange(
          { sessions: [started] } as never,
          { sessions: [] } as never,
        );
        return started;
      });
      api.setSession.mockImplementation(async () => {
        // The store reports the session again once the flag is clear: only the taken set keeps it out.
        onTerminalChange(
          { sessions: [started] } as never,
          { sessions: [] } as never,
        );
        return null;
      });
      api.snapshot.mockResolvedValue(teamWithSessions({ reviewer: "s2" }));

      expect(await store().reviveMember("reviewer")).toBe(true);

      expect(api.addMember).not.toHaveBeenCalled();
      expect(api.setSession).toHaveBeenCalledWith("reviewer", "s2");
    });

    it("closes the new session and keeps the old one when the backend refuses the tie", async () => {
      await closedReviewer();
      vi.mocked(agentLine).mockResolvedValue("claude");
      terminal.create.mockResolvedValue(session("s2"));
      api.setSession.mockRejectedValue(
        refused("the session belongs to another member"),
      );

      expect(await store().reviveMember("reviewer")).toBe(false);

      expect(terminal.close).toHaveBeenCalledWith("s2");
      expect(toast.error).toHaveBeenCalledWith(copy.team.changeRefused, {
        description: "the session belongs to another member",
      });
      expect(member("reviewer")).toMatchObject({ sessionId: "old-1" });
    });

    it("keeps the member and shows the reason when its terminal cannot start", async () => {
      await closedReviewer();
      terminal.create.mockRejectedValue(new Error("no shell"));

      expect(await store().reviveMember("reviewer")).toBe(false);

      expect(toast.error).toHaveBeenCalledWith(copy.terminal.createFailed, {
        description: "no shell",
      });
      expect(api.setSession).not.toHaveBeenCalled();
    });

    it("starts nothing for a member whose session is still open", async () => {
      terminal.sessions = [session("old-1")];
      await closedReviewer();

      expect(await store().reviveMember("reviewer")).toBe(false);

      expect(terminal.create).not.toHaveBeenCalled();
    });

    it("starts nothing on a second request while the first one runs", async () => {
      await closedReviewer();
      let finishStart: (started: TerminalInfo) => void = () => {};
      terminal.create.mockReturnValue(
        new Promise<TerminalInfo>((resolve) => {
          finishStart = resolve;
        }),
      );
      api.setSession.mockResolvedValue(null);
      api.snapshot.mockResolvedValue(teamWithSessions({ reviewer: "s2" }));

      const first = store().reviveMember("reviewer");
      expect(await store().reviveMember("reviewer")).toBe(false);
      finishStart(session("s2"));

      expect(await first).toBe(true);
      expect(terminal.create).toHaveBeenCalledTimes(1);
    });
  });
});

describe("team open terminals", () => {
  it("keeps the open terminals in the order they were opened, and closes them one at a time", () => {
    useTeamStore.setState({ openIds: [] });
    const { openTerminal, closeTerminal } = useTeamStore.getState();

    openTerminal("builder");
    openTerminal("reviewer");
    openTerminal("builder");
    expect(useTeamStore.getState().openIds).toEqual(["reviewer", "builder"]);

    closeTerminal("reviewer");
    expect(useTeamStore.getState().openIds).toEqual(["builder"]);
  });

  it("opens and closes every terminal of the worktree on the screen, and leaves the other worktrees' as they are", async () => {
    await loadTeam();
    useWorkspaceStore.setState({ selectedWorktreePath: AUTH });
    useTeamStore.setState({ openIds: ["docs-lead"] });
    const shown = membersOfWorktree(store().members, AUTH).map(
      (member) => member.id,
    );

    const frames = stubFrames();
    store().openAll();
    frames.runAll();
    expect(store().openIds).toEqual(
      expect.arrayContaining(["docs-lead", ...shown]),
    );
    expect(store().openIds).toHaveLength(shown.length + 1);

    store().closeAll();
    expect(store().openIds).toEqual(["docs-lead"]);
  });

  it("counts the terminals still to open while a batch runs", async () => {
    await loadTeam();
    useWorkspaceStore.setState({ selectedWorktreePath: AUTH });
    useTeamStore.setState({ openIds: [] });
    const shown = membersOfWorktree(store().members, AUTH).map(
      (member) => member.id,
    );
    const frames = stubFrames();

    store().openAll();
    expect(store().openingPending).toBe(shown.length);
    frames.runAll();
    expect(store().openingPending).toBe(0);
  });

  it("opens the terminals one batch per frame, and closing one before its turn keeps it closed", async () => {
    await loadTeam();
    useWorkspaceStore.setState({ selectedWorktreePath: AUTH });
    useTeamStore.setState({ openIds: [] });
    const shown = membersOfWorktree(store().members, AUTH).map(
      (member) => member.id,
    );
    const frames = stubFrames();

    store().openAll();
    expect(store().openIds).toHaveLength(0);
    frames.step();
    expect(store().openIds).toHaveLength(
      Math.min(OPEN_PER_FRAME, shown.length),
    );

    const last = shown[shown.length - 1];
    store().closeTerminal(last);
    frames.runAll();
    expect(store().openIds).not.toContain(last);
    expect(store().openIds).toHaveLength(shown.length - 1);
  });
});
