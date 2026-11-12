import {
  DotsThreeIcon,
  LockIcon,
  StarIcon,
  TerminalWindowIcon,
  TrashIcon,
  TreeStructureIcon,
  WarningIcon,
} from "@phosphor-icons/react";

import type { WorktreeInfo } from "@/bindings";
import { TooltipIconButton } from "@/components/TooltipIconButton";
import {
  ContextMenu,
  ContextMenuContent,
  ContextMenuItem,
  ContextMenuTrigger,
} from "@/components/ui/context-menu";
import {
  DropdownMenu,
  DropdownMenuContent,
  DropdownMenuItem,
  DropdownMenuTrigger,
} from "@/components/ui/dropdown-menu";
import { useDialogStore } from "@/dialog-store";
import { cn } from "@/lib/utils";
import { teamModeOf, useTeamStore } from "@/team/team-store";
import { copy } from "@/ui-copy";
import { StateDot } from "@/workspace/worktrees/StateDot";
import { worktreeLabel } from "@/workspace/worktrees/worktree-label";
import { useWorkspaceStore } from "@/workspace/workspace-store";

interface WorktreeRowProps {
  repoId: string;
  worktree: WorktreeInfo;
  selected: boolean;
  runningCount: number;
  onSelect: () => void;
}

/** Same content in the three-dot menu and in the right-click menu. */
function RemoveWorktreeItem() {
  return (
    <>
      <TrashIcon aria-hidden />
      {copy.worktree.remove}
    </>
  );
}

/**
 * The worktree's own line. It draws no background, selected or hovered: the
 * surrounding `WorktreeGroup` paints one surface for the line and its agents.
 * Its mode button switches this worktree between its terminals and its team.
 */
export function WorktreeRow({
  repoId,
  worktree,
  selected,
  runningCount,
  onSelect,
}: WorktreeRowProps) {
  const askRemoveWorktree = useDialogStore((state) => state.askRemoveWorktree);
  const mode = useTeamStore((state) =>
    teamModeOf(state.modeByWorktree, worktree.path),
  );
  const label = worktreeLabel(worktree);
  const inTeam = mode === "team";

  const row = (
    <div className="group/row flex items-center">
      <button
        type="button"
        data-nav-item
        aria-current={selected ? "true" : undefined}
        onClick={onSelect}
        className={cn(
          "flex h-7 min-w-0 flex-1 items-center gap-2 rounded-md pr-1 pl-7 text-left text-ui",
          selected
            ? "text-sidebar-accent-foreground"
            : "text-muted-foreground hover:text-sidebar-foreground",
        )}
      >
        <StateDot state={runningCount > 0 ? "running" : "idle"} />
        <span
          className={cn(
            "min-w-0 truncate",
            label.detached && "italic",
            selected && "font-medium",
          )}
          title={label.text}
        >
          {label.text}
        </span>
        {worktree.isMain && (
          <StarIcon
            weight="fill"
            className="size-3 shrink-0 text-muted-foreground"
            aria-label={copy.worktree.primary}
          />
        )}
        <span className="sr-only">
          {runningCount > 0
            ? copy.worktree.runningTerminals(runningCount)
            : copy.worktree.noTerminals}
        </span>
        {worktree.prunable && (
          <WarningIcon
            className="size-3 shrink-0 text-destructive"
            aria-label={copy.worktree.missingDirectory}
          />
        )}
        {worktree.locked && (
          <LockIcon
            className="size-3 shrink-0"
            aria-label={copy.worktree.locked}
          />
        )}
      </button>

      <div className="flex shrink-0 items-center">
        {/* Before the mode button, so that button sits in the same column on every row. */}
        {!worktree.isMain && (
          <DropdownMenu>
            <DropdownMenuTrigger asChild>
              <TooltipIconButton
                label={copy.worktree.actions(label.text)}
                className="opacity-0 transition-opacity group-focus-within/row:opacity-100 group-hover/row:opacity-100 data-[state=open]:opacity-100"
              >
                <DotsThreeIcon />
              </TooltipIconButton>
            </DropdownMenuTrigger>
            <DropdownMenuContent align="end" className="min-w-44">
              <DropdownMenuItem
                variant="destructive"
                onSelect={() => askRemoveWorktree({ repoId, worktree })}
              >
                <RemoveWorktreeItem />
              </DropdownMenuItem>
            </DropdownMenuContent>
          </DropdownMenu>
        )}
        <TooltipIconButton
          label={inTeam ? copy.team.switchToTerminals : copy.team.switchToTeam}
          onClick={() => {
            useTeamStore
              .getState()
              .setMode(worktree.path, inTeam ? "terminal" : "team");
            // The mode is the view of the selected worktree: the one switched comes to the front.
            useWorkspaceStore.getState().selectWorktree(worktree.path);
          }}
        >
          {inTeam ? <TreeStructureIcon /> : <TerminalWindowIcon />}
        </TooltipIconButton>
      </div>
    </div>
  );

  // The main worktree cannot be removed, so it has no actions of its own.
  if (worktree.isMain) return row;

  return (
    <ContextMenu>
      <ContextMenuTrigger asChild>{row}</ContextMenuTrigger>
      <ContextMenuContent className="min-w-44">
        <ContextMenuItem
          variant="destructive"
          onSelect={() => askRemoveWorktree({ repoId, worktree })}
        >
          <RemoveWorktreeItem />
        </ContextMenuItem>
      </ContextMenuContent>
    </ContextMenu>
  );
}
