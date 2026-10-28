import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { toast } from "sonner";

import type { TeamSnapshot } from "@/bindings";
import { IpcRequestError } from "@/lib/ipc-result";
import { onTeamChanged, teamBackend } from "@/team/team-backend";
import { useTeamStore, watchTeam } from "@/team/team-store";
import { copy } from "@/ui-copy";

vi.mock("sonner", () => ({ toast: { error: vi.fn() } }));
// The mock backend's live fields are not under test here: the store runs as the real app does.
vi.mock("@/dev/mock-mode", () => ({ isMockBackend: () => false }));
vi.mock("@/team/team-backend", () => ({
  teamBackend: {
    snapshot: vi.fn(),
    createRun: vi.fn(),
    deleteRun: vi.fn(),
    addMember: vi.fn(),
    setParent: vi.fn(),
    setOrchestrator: vi.fn(),
    setRole: vi.fn(),
    setTask: vi.fn(),
    dismissMember: vi.fn(),
    setLinked: vi.fn(),
  },
  onTeamChanged: vi.fn(),
}));

const api = vi.mocked(teamBackend);
const initialState = useTeamStore.getState();

const snapshot = (overrides: Partial<TeamSnapshot> = {}): TeamSnapshot => ({
  runs: [
    { id: "auth", title: "Auth refactor" },
    { id: "docs", title: "Docs rewrite" },
  ],
  members: [
    {
      id: "lead",
      runId: "auth",
      kind: "agent",
      role: "Lead",
      harness: "claude",
      botId: null,
      parentId: null,
      orchestrator: true,
      task: "",
      acceptance: "",
    },
    {
      id: "builder",
      runId: "auth",
      kind: "agent",
      role: "Builder",
      harness: "codex",
      botId: null,
      parentId: "lead",
      orchestrator: true,
      task: "login form",
      acceptance: "The form submits.",
    },
    {
      id: "reviewer",
      runId: "auth",
      kind: "agent",
      role: "Reviewer",
      harness: "claude",
      botId: null,
      parentId: null,
      orchestrator: false,
      task: "",
      acceptance: "",
    },
    {
      id: "docs-lead",
      runId: "docs",
      kind: "bot",
      role: "Lead",
      harness: null,
      botId: "lead",
      parentId: null,
      orchestrator: true,
      task: "",
      acceptance: "",
    },
  ],
  pairs: [{ a: "builder", b: "reviewer" }],
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

beforeEach(() => {
  vi.resetAllMocks();
  useTeamStore.setState(initialState, true);
});

describe("team load", () => {
  it("shows the runs, members and lines the backend sends, with the first run on screen", async () => {
    await loadTeam();

    expect(store()).toMatchObject({
      loaded: true,
      runId: "auth",
      runs: [{ id: "auth" }, { id: "docs" }],
      pairs: [{ from: "builder", to: "reviewer" }],
    });
    expect(member("builder")).toMatchObject({
      kind: "agent",
      harness: "codex",
      parentId: "lead",
      status: "idle",
      output: [],
    });
    expect(member("docs-lead")).toMatchObject({ kind: "bot", botId: "lead" });
  });

  it("keeps the run on screen while it exists, and falls back to the first run once it is gone", async () => {
    await loadTeam();
    store().setRun("docs");

    await loadTeam(snapshot());
    expect(store().runId).toBe("docs");

    await loadTeam(
      snapshot({ runs: [{ id: "auth", title: "Auth refactor" }] }),
    );
    expect(store().runId).toBe("auth");
  });

  it("drops a read that a newer read overtakes", async () => {
    let finishOlder: (team: TeamSnapshot) => void = () => {};
    api.snapshot
      .mockReturnValueOnce(
        new Promise<TeamSnapshot>((resolve) => {
          finishOlder = resolve;
        }),
      )
      .mockResolvedValueOnce(snapshot({ runs: [] }));

    const older = store().load();
    await store().load();
    finishOlder(snapshot());
    await older;

    expect(store().runs).toEqual([]);
  });

  it("shows a message and keeps the screen when the backend cannot send the team", async () => {
    api.snapshot.mockRejectedValue(new Error("the database is locked"));

    await store().load();

    expect(store().loaded).toBe(false);
    expect(toast.error).toHaveBeenCalledWith(copy.team.loadFailed, {
      description: "the database is locked",
    });
  });

  it("refuses a member the backend sends with no agent or bot, rather than guessing one", async () => {
    await loadTeam(
      snapshot({
        members: [{ ...snapshot().members[0], harness: null }],
      }),
    );

    expect(store().loaded).toBe(false);
    expect(toast.error).toHaveBeenCalled();
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
  });

  it("sends a role change to the backend, and shows nothing until the team is read again", async () => {
    api.setRole.mockResolvedValue(null);

    await store().setRole("builder", "Tester");

    expect(api.setRole).toHaveBeenCalledWith("builder", "Tester");
    expect(member("builder")?.role).toBe("Builder");
    expect(toast.error).not.toHaveBeenCalled();
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

  it("shows a member under its new orchestrator, unfolded, once the backend accepts it", async () => {
    api.setParent.mockResolvedValue(null);

    await store().setParent("reviewer", "builder");

    expect(store().expanded).toContain("builder");
    expect(store().extraRows.reviewer).toBe(0);
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
      task: "check it",
      acceptance: "",
    });

    expect(added).toBe(true);
    expect(api.addMember).toHaveBeenCalledWith(
      "auth",
      expect.objectContaining({ role: "Scout", parentId: "lead" }),
    );
    expect(store()).toMatchObject({ selectedId: "scout", addOpen: false });
    expect(store().expanded).toContain("lead");
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
      task: "",
      acceptance: "",
    });

    expect(added).toBe(false);
    expect(toast.error).toHaveBeenCalledWith(copy.team.changeRefused, {
      description: "only an orchestrator can have members under it",
    });
    expect(store().selectedId).toBeNull();
  });

  it("starts a run and shows it", async () => {
    api.createRun.mockResolvedValue({ id: "release", title: "Release notes" });

    const started = await store().addRun("Release notes");

    expect(started).toBe(true);
    expect(api.createRun).toHaveBeenCalledWith("Release notes");
    expect(store().runId).toBe("release");
  });

  it("deletes a run through the backend", async () => {
    api.deleteRun.mockResolvedValue(null);

    await store().deleteRun("docs");

    expect(api.deleteRun).toHaveBeenCalledWith("docs");
  });

  it("closes the delete confirmation once the run is gone, and keeps it open when refused", async () => {
    store().setDeleteRunOpen(true);
    api.deleteRun.mockRejectedValueOnce(refused("the run is busy"));

    await store().deleteRun("docs");
    expect(store().deleteRunOpen).toBe(true);

    api.deleteRun.mockResolvedValueOnce(null);
    await store().deleteRun("docs");
    expect(store().deleteRunOpen).toBe(false);
  });

  it("changes a member's task through the backend, and says so when it refuses the change", async () => {
    api.setTask.mockResolvedValue(null);

    expect(
      await store().setTask("builder", "login form", "The form submits."),
    ).toBe(true);
    expect(api.setTask).toHaveBeenCalledWith(
      "builder",
      "login form",
      "The form submits.",
    );

    api.setTask.mockRejectedValue(
      refused("a member with no orchestrator has no task"),
    );
    expect(await store().setTask("reviewer", "review", "approved")).toBe(false);
    expect(toast.error).toHaveBeenCalledWith(copy.team.changeRefused, {
      description: "a member with no orchestrator has no task",
    });
  });
});

describe("team expansion", () => {
  /** An in-memory localStorage: the tests run without a browser. */
  function storageWith(
    setItem: (key: string, value: string) => void = () => {},
  ) {
    const items = new Map<string, string>();
    return {
      getItem: (key: string) => items.get(key) ?? null,
      setItem: (key: string, value: string) => {
        setItem(key, value);
        items.set(key, value);
      },
    };
  }

  /** The store as a page load makes it: a fresh module, which reads what was stored. */
  async function loadedAfterReload() {
    vi.resetModules();
    const fresh = await import("@/team/team-store");
    const backend = await import("@/team/team-backend");
    vi.mocked(backend.teamBackend.snapshot).mockResolvedValue(snapshot());
    await fresh.useTeamStore.getState().load();
    return fresh.useTeamStore;
  }

  afterEach(() => {
    vi.unstubAllGlobals();
  });

  it("keeps a member the user folded folded when the team is read after a reload", async () => {
    vi.stubGlobal("localStorage", storageWith());

    const first = await loadedAfterReload();
    expect(first.getState().expanded).toContain("lead");
    first.getState().toggle("lead");

    const second = await loadedAfterReload();
    expect(second.getState().expanded).not.toContain("lead");
  });

  it("still folds a member when storage refuses to save", async () => {
    vi.stubGlobal(
      "localStorage",
      storageWith(() => {
        throw new Error("storage is full");
      }),
    );

    const blocked = await loadedAfterReload();
    blocked.getState().toggle("lead");

    expect(blocked.getState().expanded).not.toContain("lead");
  });
});
