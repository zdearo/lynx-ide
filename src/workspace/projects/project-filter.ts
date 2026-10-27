import type { Repo, TerminalInfo, WorktreeInfo } from "@/bindings";
import type { ProjectNode } from "@/workspace/projects/project-tree";

/** Every project, or only the ones with a terminal running in one of their worktrees. */
export type ProjectFilter = "all" | "active";

/** The repos with a running terminal (a shell or an agent) in any of their worktrees. */
export function activeRepoIds(
  repos: readonly Repo[],
  worktrees: Record<string, WorktreeInfo[]>,
  sessions: readonly TerminalInfo[],
): Set<string> {
  const running = new Set(
    sessions
      .filter((session) => session.running)
      .map((session) => session.worktreePath),
  );
  return new Set(
    repos
      .filter((repo) =>
        (worktrees[repo.id] ?? []).some((worktree) =>
          running.has(worktree.path),
        ),
      )
      .map((repo) => repo.id),
  );
}

/** The tree with only the active repos, and only the folders that hold one at any depth. */
export function onlyActive(
  nodes: readonly ProjectNode[],
  active: ReadonlySet<string>,
): ProjectNode[] {
  return nodes.flatMap((node): ProjectNode[] => {
    if (node.kind === "repo") return active.has(node.repo.id) ? [node] : [];
    const children = onlyActive(node.children, active);
    return children.length > 0 ? [{ ...node, children }] : [];
  });
}
