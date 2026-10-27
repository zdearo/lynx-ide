import { describe, expect, it } from "vitest";

import type { Folder, Repo, TerminalInfo, WorktreeInfo } from "@/bindings";
import { activeRepoIds, onlyActive } from "@/workspace/projects/project-filter";
import {
  buildProjectTree,
  type ProjectNode,
} from "@/workspace/projects/project-tree";

const folder = (
  id: string,
  name: string,
  parentId: string | null = null,
): Folder => ({
  id,
  name,
  parentId,
  collapsed: false,
  sourcePath: null,
  linkedRootId: null,
  missing: false,
});
const repo = (id: string, folderId: string | null = null): Repo => ({
  id,
  name: id,
  path: `/work/${id}`,
  folderId,
  linkedRootId: null,
  missing: false,
});
const worktree = (path: string): WorktreeInfo => ({
  path,
  name: path,
  branch: "main",
  head: null,
  baseRef: null,
  isMain: false,
  locked: false,
  prunable: false,
});
const session = (worktreePath: string, running: boolean): TerminalInfo => ({
  id: `${worktreePath}-${running}`,
  worktreePath,
  command: null,
  shell: "zsh",
  running,
  hibernated: false,
  exitCode: running ? null : 0,
  startedAt: 0,
  harness: null,
  agentStatus: null,
});

/** The tree as nested names, so a test reads the way the sidebar would show it. */
const shape = (nodes: ProjectNode[]): unknown[] =>
  nodes.map((node) =>
    node.kind === "repo"
      ? node.repo.id
      : [node.folder.name, ...shape(node.children)],
  );

describe("activeRepoIds", () => {
  const repos = [repo("a"), repo("b")];
  const worktrees = {
    a: [worktree("/work/a"), worktree("/work/a-feat")],
    b: [worktree("/work/b")],
  };

  it("counts a repo once any of its worktrees has a running terminal", () => {
    const active = activeRepoIds(repos, worktrees, [
      session("/work/a-feat", true),
    ]);
    expect([...active]).toEqual(["a"]);
  });

  it("does not count a terminal that has exited", () => {
    const active = activeRepoIds(repos, worktrees, [session("/work/b", false)]);
    expect(active.size).toBe(0);
  });
});

describe("onlyActive", () => {
  it("keeps a folder only while it holds an active repo at some depth", () => {
    const tree = buildProjectTree(
      [
        folder("f1", "Work"),
        folder("f2", "Inner", "f1"),
        folder("f3", "Quiet"),
      ],
      [repo("a", "f2"), repo("b", "f1"), repo("c", "f3"), repo("d"), repo("e")],
    );

    expect(shape(onlyActive(tree, new Set(["a", "e"])))).toEqual([
      ["Work", ["Inner", "a"]],
      "e",
    ]);
  });

  it("shows nothing when no repo is active", () => {
    const tree = buildProjectTree([folder("f1", "Work")], [repo("a", "f1")]);
    expect(onlyActive(tree, new Set())).toEqual([]);
  });
});
