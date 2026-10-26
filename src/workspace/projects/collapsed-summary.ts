import type { TerminalInfo, WorktreeInfo } from "@/bindings";
import type { ProjectNode } from "@/workspace/projects/project-tree";

/** What a collapsed row hides: its projects and worktrees, and the agents inside them. */
export interface GroupSummary {
  projects: number;
  worktrees: number;
  runningAgents: number;
  /** Running agents waiting for the user; a subset of `runningAgents`. */
  needingYou: number;
}

/** A worktree's running agents, and how many of them are waiting for the user. */
export interface AgentCount {
  running: number;
  needingYou: number;
}

const NOTHING: GroupSummary = {
  projects: 0,
  worktrees: 0,
  runningAgents: 0,
  needingYou: 0,
};

/** Running agent sessions per worktree path. Plain shells do not count: they are not agents. */
export function agentsByWorktree(
  sessions: readonly TerminalInfo[],
): Map<string, AgentCount> {
  const counts = new Map<string, AgentCount>();
  for (const session of sessions) {
    if (!session.running || session.harness === null) continue;
    const count = counts.get(session.worktreePath) ?? {
      running: 0,
      needingYou: 0,
    };
    count.running += 1;
    if (session.agentStatus === "needsYou") count.needingYou += 1;
    counts.set(session.worktreePath, count);
  }
  return counts;
}

/** A repo's own summary: its worktrees and the agents running in them. */
export function summarizeRepo(
  worktrees: readonly WorktreeInfo[],
  agents: ReadonlyMap<string, AgentCount>,
): GroupSummary {
  return worktrees.reduce<GroupSummary>((total, worktree) => {
    const count = agents.get(worktree.path);
    return {
      ...total,
      worktrees: total.worktrees + 1,
      runningAgents: total.runningAgents + (count?.running ?? 0),
      needingYou: total.needingYou + (count?.needingYou ?? 0),
    };
  }, NOTHING);
}

/** Everything inside a folder, at any depth: each repo counts as one project. */
export function summarizeChildren(
  nodes: readonly ProjectNode[],
  worktreesByRepo: Record<string, WorktreeInfo[]>,
  agents: ReadonlyMap<string, AgentCount>,
): GroupSummary {
  return nodes.reduce<GroupSummary>((total, node) => {
    const part =
      node.kind === "repo"
        ? {
            ...summarizeRepo(worktreesByRepo[node.repo.id] ?? [], agents),
            projects: 1,
          }
        : summarizeChildren(node.children, worktreesByRepo, agents);
    return {
      projects: total.projects + part.projects,
      worktrees: total.worktrees + part.worktrees,
      runningAgents: total.runningAgents + part.runningAgents,
      needingYou: total.needingYou + part.needingYou,
    };
  }, NOTHING);
}
