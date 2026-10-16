import { beforeEach, describe, expect, it, vi } from "vitest";

import { commands } from "@/bindings";
import { failure, ok, worktree } from "@/test-support";
import { useWorkspaceStore } from "@/workspace/workspace-store";

vi.mock("@/bindings", () => ({
  commands: {
    listRepos: vi.fn(),
    listFolders: vi.fn(),
    createFolder: vi.fn(),
    renameFolder: vi.fn(),
    moveFolder: vi.fn(),
    deleteFolder: vi.fn(),
    setFolderCollapsed: vi.fn(),
    moveRepo: vi.fn(),
    addRepo: vi.fn(),
    createProject: vi.fn(),
    removeRepo: vi.fn(),
    listWorktrees: vi.fn(),
    createWorktree: vi.fn(),
    removeWorktree: vi.fn(),
  },
}));

const api = vi.mocked(commands);
const initialState = useWorkspaceStore.getState();

const repoA = { id: "a", name: "api", path: "/work/api", folderId: null };
const repoB = { id: "b", name: "web", path: "/work/web", folderId: null };
const mainA = worktree("/work/api", { isMain: true, branch: "main" });
const mainB = worktree("/work/web", { isMain: true, branch: "main" });

beforeEach(() => {
  vi.resetAllMocks();
  api.listFolders.mockResolvedValue(ok([]));
  useWorkspaceStore.setState(initialState, true);
});

function listsByRepo(lists: Record<string, ReturnType<typeof worktree>[]>) {
  api.listWorktrees.mockImplementation(async (repoId: string) =>
    ok(lists[repoId] ?? []),
  );
}

describe("load", () => {
  it("loads repos and their worktrees, then selects the first main worktree", async () => {
    api.listRepos.mockResolvedValue(ok([repoA, repoB]));
    listsByRepo({ a: [mainA], b: [mainB] });

    await useWorkspaceStore.getState().load();

    const state = useWorkspaceStore.getState();
    expect(state.loaded).toBe(true);
    expect(state.repos).toEqual([repoA, repoB]);
    expect(state.worktrees.b).toEqual([mainB]);
    expect(state.selectedWorktreePath).toBe("/work/api");
  });

  it("keeps a selection that still exists", async () => {
    api.listRepos.mockResolvedValue(ok([repoA]));
    const feature = worktree("/work/api-worktrees/feature");
    listsByRepo({ a: [mainA, feature] });
    useWorkspaceStore.setState({ selectedWorktreePath: feature.path });

    await useWorkspaceStore.getState().load();

    expect(useWorkspaceStore.getState().selectedWorktreePath).toBe(
      feature.path,
    );
  });

  it("drops a remembered selection whose worktree is gone", async () => {
    api.listRepos.mockResolvedValue(ok([repoA]));
    listsByRepo({ a: [mainA] });
    useWorkspaceStore.setState({ selectedWorktreePath: "/gone" });

    await useWorkspaceStore.getState().load();

    expect(useWorkspaceStore.getState().selectedWorktreePath).toBe("/work/api");
  });

  it("selects nothing when there are no repos", async () => {
    api.listRepos.mockResolvedValue(ok([]));

    await useWorkspaceStore.getState().load();

    expect(useWorkspaceStore.getState().selectedWorktreePath).toBeNull();
  });
});

describe("refreshWorktrees", () => {
  it("keeps the last known list when the backend rejects the refresh", async () => {
    useWorkspaceStore.setState({ repos: [repoA], worktrees: { a: [mainA] } });
    api.listWorktrees.mockResolvedValue(failure("git", "folder vanished"));

    await useWorkspaceStore.getState().refreshWorktrees("a");

    expect(useWorkspaceStore.getState().worktrees.a).toEqual([mainA]);
  });
});

describe("createProject", () => {
  it("registers the new repo like an opened one and selects its main worktree", async () => {
    const created = {
      id: "c",
      name: "notes",
      path: "/work/notes",
      folderId: null,
    };
    api.createProject.mockResolvedValue(ok(created));
    listsByRepo({
      c: [worktree("/work/notes", { isMain: true, branch: "main" })],
    });

    await useWorkspaceStore.getState().createProject("/work", "notes");

    expect(api.createProject).toHaveBeenCalledWith("/work", "notes");
    const state = useWorkspaceStore.getState();
    expect(state.repos).toEqual([created]);
    expect(state.selectedWorktreePath).toBe("/work/notes");
  });

  it("rejects with the backend's error and adds nothing", async () => {
    api.createProject.mockResolvedValue(
      failure("alreadyExists", "/work/notes already exists"),
    );

    await expect(
      useWorkspaceStore.getState().createProject("/work", "notes"),
    ).rejects.toMatchObject({ kind: "alreadyExists" });
    expect(useWorkspaceStore.getState().repos).toEqual([]);
  });
});

describe("addRepo", () => {
  it("adds the repo once and selects its main worktree", async () => {
    api.addRepo.mockResolvedValue(ok(repoA));
    listsByRepo({ a: [mainA] });

    await useWorkspaceStore.getState().addRepo("/work/api");
    await useWorkspaceStore.getState().addRepo("/work/api/src");

    const state = useWorkspaceStore.getState();
    expect(state.repos).toEqual([repoA]);
    expect(state.selectedWorktreePath).toBe("/work/api");
  });

  it("rejects with the backend's error and changes nothing", async () => {
    api.addRepo.mockResolvedValue(
      failure("notAGitRepo", "/tmp is not a git repository"),
    );

    await expect(
      useWorkspaceStore.getState().addRepo("/tmp"),
    ).rejects.toMatchObject({
      kind: "notAGitRepo",
    });
    expect(useWorkspaceStore.getState().repos).toEqual([]);
  });
});

describe("removeRepo", () => {
  it("forgets the repo and moves the selection off its worktrees", async () => {
    useWorkspaceStore.setState({
      repos: [repoA, repoB],
      worktrees: { a: [mainA], b: [mainB] },
      selectedWorktreePath: "/work/api",
    });
    api.removeRepo.mockResolvedValue(ok(null));

    await useWorkspaceStore.getState().removeRepo("a");

    const state = useWorkspaceStore.getState();
    expect(state.repos).toEqual([repoB]);
    expect(state.worktrees.a).toBeUndefined();
    expect(state.selectedWorktreePath).toBe("/work/web");
  });
});

describe("createWorktree", () => {
  const draft = {
    repoId: "a",
    name: " feature/x ",
    baseRef: "main",
    error: null,
  };

  it("closes the dialog at once, shows a progress row, then selects the new worktree", async () => {
    useWorkspaceStore.setState({
      repos: [repoA],
      worktrees: { a: [mainA] },
      draft,
    });
    const created = worktree("/work/api-worktrees/feature-x", {
      branch: "feature/x",
    });
    let finish: (value: ReturnType<typeof ok>) => void = () => {};
    api.createWorktree.mockReturnValue(
      new Promise((resolve) => {
        finish = resolve as typeof finish;
      }),
    );
    listsByRepo({ a: [mainA, created] });

    const creating = useWorkspaceStore.getState().createWorktree(draft);

    let state = useWorkspaceStore.getState();
    expect(state.draft).toBeNull();
    expect(state.pending).toHaveLength(1);
    expect(state.pending[0]).toMatchObject({ repoId: "a", name: "feature/x" });

    finish(ok(created));
    await creating;

    state = useWorkspaceStore.getState();
    expect(state.pending).toEqual([]);
    expect(state.worktrees.a).toEqual([mainA, created]);
    expect(state.selectedWorktreePath).toBe(created.path);
    expect(api.createWorktree).toHaveBeenCalledWith("a", " feature/x ", "main");
  });

  it("reopens the dialog with what was typed and the reason when Git refuses", async () => {
    useWorkspaceStore.setState({ repos: [repoA], worktrees: { a: [mainA] } });
    api.createWorktree.mockResolvedValue(
      failure("alreadyExists", "branch feature/x already exists"),
    );

    await useWorkspaceStore.getState().createWorktree(draft);

    const state = useWorkspaceStore.getState();
    expect(state.pending).toEqual([]);
    expect(state.draft).toEqual({
      ...draft,
      error: "branch feature/x already exists",
    });
    expect(state.selectedWorktreePath).toBeNull();
  });

  it("expands a collapsed repo so the progress row is visible", async () => {
    useWorkspaceStore.setState({
      repos: [repoA],
      worktrees: { a: [mainA] },
      collapsedRepoIds: { a: true },
    });
    api.createWorktree.mockResolvedValue(failure("git", "boom"));

    await useWorkspaceStore.getState().createWorktree(draft);

    expect(useWorkspaceStore.getState().collapsedRepoIds.a).toBe(false);
  });
});

describe("removeWorktree", () => {
  const feature = worktree("/work/api-worktrees/feature");

  it("refreshes the list and falls back to main when the selected worktree vanished", async () => {
    useWorkspaceStore.setState({
      repos: [repoA],
      worktrees: { a: [mainA, feature] },
      selectedWorktreePath: feature.path,
    });
    const result = {
      branch: "feature",
      branchDeleted: true,
      branchKeptReason: null,
    };
    api.removeWorktree.mockResolvedValue(ok(result));
    listsByRepo({ a: [mainA] });

    await expect(
      useWorkspaceStore.getState().removeWorktree("a", feature.path, false),
    ).resolves.toEqual(result);

    const state = useWorkspaceStore.getState();
    expect(state.worktrees.a).toEqual([mainA]);
    expect(state.selectedWorktreePath).toBe("/work/api");
  });

  it("rejects with the kind so the UI can offer a forced removal", async () => {
    api.removeWorktree.mockResolvedValue(
      failure("worktreeHasChanges", "the worktree has uncommitted changes"),
    );

    await expect(
      useWorkspaceStore.getState().removeWorktree("a", feature.path, false),
    ).rejects.toMatchObject({ kind: "worktreeHasChanges" });
  });
});

describe("repo sections and the dialog", () => {
  it("toggles a repo section", () => {
    useWorkspaceStore.getState().toggleRepo("a");
    expect(useWorkspaceStore.getState().collapsedRepoIds.a).toBe(true);
    useWorkspaceStore.getState().toggleRepo("a");
    expect(useWorkspaceStore.getState().collapsedRepoIds.a).toBe(false);
  });

  it("opens an empty draft for a repo and closes it", () => {
    useWorkspaceStore.getState().openNewWorktree("a");
    expect(useWorkspaceStore.getState().draft).toEqual({
      repoId: "a",
      name: "",
      baseRef: null,
      error: null,
    });
    useWorkspaceStore.getState().closeNewWorktree();
    expect(useWorkspaceStore.getState().draft).toBeNull();
  });
});

describe("folders", () => {
  const work = { id: "work", name: "Work", parentId: null, collapsed: false };
  const repoInWork = {
    id: "a",
    name: "api",
    path: "/work/api",
    folderId: "work",
  };

  it("loads folders together with repos", async () => {
    api.listRepos.mockResolvedValue(ok([repoA]));
    api.listFolders.mockResolvedValue(ok([work]));
    listsByRepo({ a: [mainA] });

    await useWorkspaceStore.getState().load();

    expect(useWorkspaceStore.getState().folders).toEqual([work]);
  });

  it("opens the parent when a folder is created inside a collapsed one", async () => {
    const collapsed = { ...work, collapsed: true };
    useWorkspaceStore.setState({ folders: [collapsed] });
    api.createFolder.mockResolvedValue(
      ok({ id: "x", name: "X", parentId: "work", collapsed: false }),
    );
    api.setFolderCollapsed.mockResolvedValue(ok(null));
    api.listFolders.mockResolvedValue(ok([{ ...work, collapsed: false }]));
    api.listRepos.mockResolvedValue(ok([]));

    await useWorkspaceStore.getState().createFolder("X", "work");

    expect(api.setFolderCollapsed).toHaveBeenCalledWith("work", false);
  });

  it("collapses a folder at once and re-reads the truth when the backend refuses", async () => {
    useWorkspaceStore.setState({ folders: [work] });
    api.setFolderCollapsed.mockResolvedValue(
      failure("folderNotFound", "folder not found: work"),
    );
    api.listFolders.mockResolvedValue(ok([work]));
    api.listRepos.mockResolvedValue(ok([]));

    await expect(
      useWorkspaceStore.getState().setFolderCollapsed("work", true),
    ).rejects.toMatchObject({
      kind: "folderNotFound",
    });

    expect(useWorkspaceStore.getState().folders[0].collapsed).toBe(false);
  });

  it("moves a repo and reloads the structure so the tree reflects it", async () => {
    useWorkspaceStore.setState({ folders: [work], repos: [repoA] });
    api.moveRepo.mockResolvedValue(ok(null));
    api.listRepos.mockResolvedValue(ok([repoInWork]));
    api.listFolders.mockResolvedValue(ok([work]));

    await useWorkspaceStore.getState().moveRepo("a", "work");

    expect(api.moveRepo).toHaveBeenCalledWith("a", "work");
    expect(useWorkspaceStore.getState().repos[0].folderId).toBe("work");
  });

  it("deleting a folder reloads what moved up out of it", async () => {
    useWorkspaceStore.setState({ folders: [work], repos: [repoInWork] });
    api.deleteFolder.mockResolvedValue(ok(null));
    api.listFolders.mockResolvedValue(ok([]));
    api.listRepos.mockResolvedValue(ok([{ ...repoInWork, folderId: null }]));

    await useWorkspaceStore.getState().deleteFolder("work");

    expect(useWorkspaceStore.getState().folders).toEqual([]);
    expect(useWorkspaceStore.getState().repos[0].folderId).toBeNull();
  });
});
