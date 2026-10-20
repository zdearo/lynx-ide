import { describe, expect, it } from "vitest";

import type { Folder, Repo, TerminalInfo, WorktreeInfo } from "@/bindings";
import { session, worktree } from "@/test-support";
import {
  agentsByWorktree,
  summarizeChildren,
  summarizeRepo,
} from "@/workspace/collapsed-summary";
import { buildProjectTree } from "@/workspace/project-tree";

const repo = (id: string, folderId: string | null = null): Repo => ({
  id,
  name: id,
  path: `/work/${id}`,
  folderId,
  linkedRootId: null,
  missing: false,
});
const folder = (id: string, parentId: string | null = null): Folder => ({
  id,
  name: id,
  parentId,
  collapsed: false,
  sourcePath: null,
  linkedRootId: null,
  missing: false,
});

describe("agentsByWorktree", () => {
  it("counts running agents per worktree and ignores shells and exited agents", () => {
    const sessions: TerminalInfo[] = [
      session("a", "/w/one", { command: "claude", harness: "claude" }),
      session("b", "/w/one", { command: "codex", harness: "codex" }),
      session("c", "/w/one", { command: null }),
      session("d", "/w/one", {
        command: "claude",
        harness: "claude",
        running: false,
        exitCode: 0,
      }),
      session("e", "/w/two", { command: "claude", harness: "claude" }),
    ];
    const counts = agentsByWorktree(sessions);
    expect(counts.get("/w/one")).toEqual({ running: 2, needingYou: 0 });
    expect(counts.get("/w/two")).toEqual({ running: 1, needingYou: 0 });
  });

  it("counts the running agents that are waiting for the user", () => {
    const sessions: TerminalInfo[] = [
      session("a", "/w/one", {
        command: "claude",
        harness: "claude",
        agentStatus: "needsYou",
      }),
      session("b", "/w/one", {
        command: "codex",
        harness: "codex",
        agentStatus: "working",
      }),
      session("c", "/w/one", {
        command: "claude",
        harness: "claude",
        agentStatus: "needsYou",
        running: false,
        exitCode: 0,
      }),
    ];
    expect(agentsByWorktree(sessions).get("/w/one")).toEqual({
      running: 2,
      needingYou: 1,
    });
  });
});

describe("summarizeRepo", () => {
  it("counts its worktrees and the agents running in them", () => {
    const worktrees: WorktreeInfo[] = [
      worktree("/w/main"),
      worktree("/w/task"),
    ];
    const agents = new Map([["/w/task", { running: 2, needingYou: 1 }]]);
    expect(summarizeRepo(worktrees, agents)).toEqual({
      projects: 0,
      worktrees: 2,
      runningAgents: 2,
      needingYou: 1,
    });
  });
});

describe("summarizeChildren", () => {
  it("adds up every project and worktree at any depth below a folder", () => {
    const folders = [folder("work"), folder("client", "work")];
    const repos = [repo("api", "client"), repo("site", "work")];
    const tree = buildProjectTree(folders, repos);
    const worktreesByRepo = {
      api: [worktree("/w/api"), worktree("/w/api-task")],
      site: [worktree("/w/site")],
    };
    const agents = new Map([["/w/api-task", { running: 1, needingYou: 0 }]]);

    const top = tree[0];
    if (top.kind !== "folder") throw new Error("expected a folder");
    expect(summarizeChildren(top.children, worktreesByRepo, agents)).toEqual({
      projects: 2,
      worktrees: 3,
      runningAgents: 1,
      needingYou: 0,
    });
  });

  it("is empty for an empty folder", () => {
    expect(summarizeChildren([], {}, new Map())).toEqual({
      projects: 0,
      worktrees: 0,
      runningAgents: 0,
      needingYou: 0,
    });
  });
});
