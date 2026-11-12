import type { Repo, TerminalInfo, WorktreeInfo } from "@/bindings";
import type { ProjectNode } from "@/workspace/projects/project-tree";
import { worktreeLabel } from "@/workspace/worktrees/worktree-label";

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

/**
 * The tree narrowed to a search, with the worktrees to show for each repo. A repo whose name contains the query shows
 * all its worktrees; otherwise only the worktrees whose branch does. Folders stay only where a match is inside them.
 */
export function searchProjects(
  nodes: readonly ProjectNode[],
  worktrees: Record<string, WorktreeInfo[]>,
  query: string,
): { nodes: ProjectNode[]; worktrees: Record<string, WorktreeInfo[]> } {
  const needle = query.trim().toLowerCase();
  if (needle === "") return { nodes: [...nodes], worktrees };
  const matches = (text: string) => text.toLowerCase().includes(needle);
  const shown = { ...worktrees };
  const walk = (list: readonly ProjectNode[]): ProjectNode[] =>
    list.flatMap((node): ProjectNode[] => {
      if (node.kind === "folder") {
        const children = walk(node.children);
        return children.length > 0 ? [{ ...node, children }] : [];
      }
      const own = worktrees[node.repo.id] ?? [];
      const repoMatches = matches(node.repo.name);
      const kept = repoMatches
        ? own
        : own.filter((worktree) => matches(worktreeLabel(worktree).text));
      shown[node.repo.id] = kept;
      return repoMatches || kept.length > 0 ? [node] : [];
    });
  return { nodes: walk(nodes), worktrees: shown };
}
