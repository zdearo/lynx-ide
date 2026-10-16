import { useMemo } from "react";
import { ChevronRight, MoreHorizontal, Plus } from "lucide-react";

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
import { useTerminalStore } from "@/terminal/terminal-store";
import { copy } from "@/ui-copy";
import { StateDot } from "@/workspace/StateDot";
import { WorktreeRow } from "@/workspace/WorktreeRow";
import {
  useWorkspaceStore,
  type PendingWorktree,
} from "@/workspace/workspace-store";

interface RepoSectionProps {
  repo: Repo;
  worktrees: WorktreeInfo[];
  pending: PendingWorktree[];
  collapsed: boolean;
  selectedPath: string | null;
  onSelectWorktree: (path: string) => void;
}

export function RepoSection({
  repo,
  worktrees,
  pending,
  collapsed,
  selectedPath,
  onSelectWorktree,
}: RepoSectionProps) {
  const toggleRepo = useWorkspaceStore((state) => state.toggleRepo);
  const openNewWorktree = useWorkspaceStore((state) => state.openNewWorktree);
  const refreshWorktrees = useWorkspaceStore((state) => state.refreshWorktrees);
  const askRemoveRepo = useDialogStore((state) => state.askRemoveRepo);
  const sessions = useTerminalStore((state) => state.sessions);

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
          onClick={() => toggleRepo(repo.id)}
          className="flex h-7 w-full items-center gap-1.5 rounded-md px-1.5 text-left text-ui font-medium text-sidebar-foreground hover:bg-sidebar-accent/50"
        >
          <ChevronRight
            aria-hidden
            className={cn(
              "size-3.5 shrink-0 text-muted-foreground transition-transform",
              !collapsed && "rotate-90",
            )}
          />
          <span className="min-w-0 flex-1 truncate" title={repo.path}>
            {repo.name}
          </span>
        </button>
        <div className="absolute top-0.5 right-1 flex items-center gap-0.5 rounded-md bg-sidebar opacity-0 transition-opacity group-focus-within/repo:opacity-100 group-hover/repo:opacity-100">
          <TooltipIconButton
            label={copy.repo.newWorktree}
            shortcut={shortcutLabel("newWorktree")}
            onClick={() => openNewWorktree(repo.id)}
          >
            <Plus />
          </TooltipIconButton>
          <DropdownMenu>
            <DropdownMenuTrigger asChild>
              <TooltipIconButton label={copy.repo.actions(repo.name)}>
                <MoreHorizontal />
              </TooltipIconButton>
            </DropdownMenuTrigger>
            <DropdownMenuContent align="end" className="min-w-44">
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
        </div>
      </div>

      {!collapsed && (
        <ul>
          {worktrees.map((worktree) => (
            <WorktreeRow
              key={worktree.path}
              repoId={repo.id}
              worktree={worktree}
              selected={worktree.path === selectedPath}
              runningCount={runningByPath.get(worktree.path) ?? 0}
              onSelect={() => onSelectWorktree(worktree.path)}
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
