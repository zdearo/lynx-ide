import { useMemo } from "react";
import { ChevronRightGlyph18 } from "@/components/icons/ChevronRightGlyph18";
import { CodeBranchGlyph18 } from "@/components/icons/CodeBranchGlyph18";
import { DotsGlyph18 } from "@/components/icons/DotsGlyph18";
import { PlusGlyph18 } from "@/components/icons/PlusGlyph18";

import type { Repo, WorktreeInfo } from "@/bindings";
import { TooltipIconButton } from "@/components/TooltipIconButton";
import {
  DropdownMenu,
  DropdownMenuContent,
  DropdownMenuItem,
  DropdownMenuTrigger,
} from "@/components/ui/dropdown-menu";
import { useDialogStore } from "@/dialog-store";
import { cn } from "@/lib/utils";
import { shortcutLabel } from "@/platform-shortcuts";
import { requestCloseTerminal } from "@/terminal/terminal-actions";
import { tabTitles } from "@/terminal/terminal-tab-title";
import { activeSessionId, useTerminalStore } from "@/terminal/terminal-store";
import type { AgentItem } from "@/workspace/AgentRow";
import { copy } from "@/ui-copy";
import { StateDot } from "@/workspace/StateDot";
import { CollapsedSummary } from "@/workspace/CollapsedSummary";
import type { GroupSummary } from "@/workspace/collapsed-summary";
import { WorktreeGroup } from "@/workspace/WorktreeGroup";
import {
  useWorkspaceStore,
  type PendingWorktree,
} from "@/workspace/workspace-store";

interface RepoSectionProps {
  repo: Repo;
  summary: GroupSummary;
  worktrees: WorktreeInfo[];
  pending: PendingWorktree[];
  collapsed: boolean;
  selectedPath: string | null;
  onSelectWorktree: (path: string) => void;
}

export function RepoSection({
  repo,
  summary,
  worktrees,
  pending,
  collapsed,
  selectedPath,
  onSelectWorktree,
}: RepoSectionProps) {
  const toggleRepo = useWorkspaceStore((state) => state.toggleRepo);
  const expandedAgentGroups = useWorkspaceStore(
    (state) => state.expandedAgentGroups,
  );
  const toggleAgentGroup = useWorkspaceStore((state) => state.toggleAgentGroup);
  const openNewWorktree = useWorkspaceStore((state) => state.openNewWorktree);
  const refreshWorktrees = useWorkspaceStore((state) => state.refreshWorktrees);
  const askRemoveRepo = useDialogStore((state) => state.askRemoveRepo);
  const askMove = useDialogStore((state) => state.askMove);
  const sessions = useTerminalStore((state) => state.sessions);
  const activeByWorktree = useTerminalStore((state) => state.activeByWorktree);
  const setActiveTerminal = useTerminalStore((state) => state.setActive);
  const requestFocus = useTerminalStore((state) => state.requestFocus);

  const runningByPath = useMemo(() => {
    const counts = new Map<string, number>();
    for (const session of sessions) {
      if (session.running) {
        counts.set(
          session.worktreePath,
          (counts.get(session.worktreePath) ?? 0) + 1,
        );
      }
    }
    return counts;
  }, [sessions]);

  // Agents are the sessions started from a known agent; plain shells stay in the tab bar.
  const agentsByPath = useMemo(() => {
    const grouped = new Map<string, AgentItem[]>();
    const byPath = new Map<string, typeof sessions>();
    for (const session of sessions) {
      byPath.set(session.worktreePath, [
        ...(byPath.get(session.worktreePath) ?? []),
        session,
      ]);
    }
    for (const [path, own] of byPath) {
      // Titles are numbered across all of the worktree's tabs, so they match the tab bar.
      const titles = tabTitles(own);
      grouped.set(
        path,
        own.flatMap((session) =>
          session.harness === null
            ? []
            : [
                {
                  id: session.id,
                  harness: session.harness,
                  title: titles.get(session.id) ?? session.shell,
                  running: session.running,
                  exitCode: session.exitCode,
                  startedAt: session.startedAt,
                  status: session.agentStatus,
                },
              ],
        ),
      );
    }
    return grouped;
  }, [sessions]);

  return (
    <li>
      <div className="group/repo relative">
        <button
          type="button"
          data-nav-item
          aria-expanded={!collapsed}
          aria-label={
            collapsed
              ? copy.repo.expand(repo.name)
              : copy.repo.collapse(repo.name)
          }
          data-project-id={repo.id}
          aria-describedby={collapsed ? `repo-summary-${repo.id}` : undefined}
          onClick={() => toggleRepo(repo.id)}
          className="flex h-7 w-full items-center gap-1.5 rounded-md px-1.5 text-left text-ui font-medium text-sidebar-foreground hover:bg-sidebar-accent/50"
        >
          <CodeBranchGlyph18
            aria-hidden
            className="size-3.5 shrink-0 text-muted-foreground"
          />
          <span className="min-w-0 flex-1 truncate" title={repo.path}>
            {repo.name}
          </span>
          {collapsed && (
            <CollapsedSummary
              id={`repo-summary-${repo.id}`}
              kind="repo"
              summary={summary}
            />
          )}
        </button>
        <div className="pointer-events-none absolute top-0.5 right-1 flex items-center gap-0.5 rounded-md bg-sidebar opacity-0 transition-opacity group-has-[:focus-visible]/repo:opacity-100 group-hover/repo:opacity-100">
          <TooltipIconButton
            className="pointer-events-auto"
            label={copy.repo.newWorktree}
            shortcut={shortcutLabel("newWorktree")}
            onClick={() => openNewWorktree(repo.id)}
          >
            <PlusGlyph18 />
          </TooltipIconButton>
          <DropdownMenu>
            <DropdownMenuTrigger asChild>
              <TooltipIconButton
                label={copy.repo.actions(repo.name)}
                className="pointer-events-auto"
              >
                <DotsGlyph18 />
              </TooltipIconButton>
            </DropdownMenuTrigger>
            <DropdownMenuContent align="end" className="min-w-44">
              <DropdownMenuItem
                onSelect={() => askMove({ kind: "repo", repo })}
              >
                {copy.repo.moveToFolder}
              </DropdownMenuItem>
              <DropdownMenuItem onSelect={() => void refreshWorktrees(repo.id)}>
                {copy.repo.refresh}
              </DropdownMenuItem>
              <DropdownMenuItem
                variant="destructive"
                onSelect={() => askRemoveRepo(repo)}
              >
                {copy.repo.removeFromList}
              </DropdownMenuItem>
            </DropdownMenuContent>
          </DropdownMenu>
          <span
            aria-hidden
            className="pointer-events-none grid size-6 place-items-center text-muted-foreground"
          >
            <ChevronRightGlyph18
              data-icon="chevron-right"
              className={cn(
                "size-3.5 transition-transform",
                !collapsed && "rotate-90",
              )}
            />
          </span>
        </div>
      </div>

      {!collapsed && (
        <ul className="grid grid-cols-1 gap-1">
          {worktrees.map((worktree) => (
            <WorktreeGroup
              key={worktree.path}
              repoId={repo.id}
              worktree={worktree}
              selected={worktree.path === selectedPath}
              runningCount={runningByPath.get(worktree.path) ?? 0}
              agents={agentsByPath.get(worktree.path) ?? []}
              // Only the selected worktree has an open tab on screen; the remembered one of
              // the others must not look selected.
              activeAgentId={
                worktree.path === selectedPath
                  ? activeSessionId(sessions, activeByWorktree, worktree.path)
                  : null
              }
              agentsExpanded={expandedAgentGroups[worktree.path] ?? false}
              onToggleAgents={() => toggleAgentGroup(worktree.path)}
              onSelect={() => onSelectWorktree(worktree.path)}
              onOpenAgent={(id) => {
                // The agent may belong to a worktree that is not on screen yet.
                if (worktree.path !== selectedPath) {
                  onSelectWorktree(worktree.path);
                }
                setActiveTerminal(worktree.path, id);
                requestFocus(id);
              }}
              onCloseAgent={requestCloseTerminal}
            />
          ))}
          {pending.map((item) => (
            <li
              key={item.id}
              className="flex h-7 items-center gap-2 pr-2 pl-7 text-ui text-muted-foreground"
            >
              <StateDot state="creating" />
              <span className="truncate">
                {copy.worktree.creating(item.name)}
              </span>
            </li>
          ))}
        </ul>
      )}
    </li>
  );
}
