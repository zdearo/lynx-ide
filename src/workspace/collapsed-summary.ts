import type { TerminalInfo, WorktreeInfo } from "@/bindings";
import { isAgentCommand } from "@/terminal/agent-presets";
import type { ProjectNode } from "@/workspace/project-tree";

/** What a collapsed row hides: its projects and worktrees, and the agents running in them. */
export interface GroupSummary {
  projects: number;
  worktrees: number;
  runningAgents: number;
}

const NOTHING: GroupSummary = { projects: 0, worktrees: 0, runningAgents: 0 };

/** Running agent sessions per worktree path. Plain shells do not count: they are not agents. */
export function runningAgentsByWorktree(
  sessions: readonly TerminalInfo[],
): Map<string, number> {
  const counts = new Map<string, number>();
  for (const session of sessions) {
    if (session.running && isAgentCommand(session.command)) {
      counts.set(
        session.worktreePath,
        (counts.get(session.worktreePath) ?? 0) + 1,
      );
    }
  }
  return counts;
}

/** A repo's own summary: its worktrees and the agents running in them. */
export function summarizeRepo(
  worktrees: readonly WorktreeInfo[],
  runningAgents: ReadonlyMap<string, number>,
): GroupSummary {
  return {
    projects: 0,
    worktrees: worktrees.length,
    runningAgents: worktrees.reduce(
      (total, worktree) => total + (runningAgents.get(worktree.path) ?? 0),
      0,
    ),
  };
}

/** Everything inside a folder, at any depth: each repo counts as one project. */
export function summarizeChildren(
  nodes: readonly ProjectNode[],
  worktreesByRepo: Record<string, WorktreeInfo[]>,
  runningAgents: ReadonlyMap<string, number>,
): GroupSummary {
  return nodes.reduce<GroupSummary>((total, node) => {
    const part =
      node.kind === "repo"
        ? {
            ...summarizeRepo(
              worktreesByRepo[node.repo.id] ?? [],
              runningAgents,
            ),
            projects: 1,
          }
        : summarizeChildren(node.children, worktreesByRepo, runningAgents);
    return {
      projects: total.projects + part.projects,
      worktrees: total.worktrees + part.worktrees,
      runningAgents: total.runningAgents + part.runningAgents,
    };
  }, NOTHING);
}
