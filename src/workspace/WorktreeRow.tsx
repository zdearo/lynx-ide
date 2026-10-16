import {
  Lock,
  MoreHorizontal,
  SquareTerminal,
  TriangleAlert,
} from "lucide-react";

import type { WorktreeInfo } from "@/bindings";
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
import { NewTerminalMenu } from "@/terminal/NewTerminalMenu";
import { copy } from "@/ui-copy";
import { StateDot } from "@/workspace/StateDot";
import { worktreeLabel } from "@/workspace/worktree-label";

interface WorktreeRowProps {
  repoId: string;
  worktree: WorktreeInfo;
  selected: boolean;
  runningCount: number;
  onSelect: () => void;
}

export function WorktreeRow({
  repoId,
  worktree,
  selected,
  runningCount,
  onSelect,
}: WorktreeRowProps) {
  const askRemoveWorktree = useDialogStore((state) => state.askRemoveWorktree);
  const label = worktreeLabel(worktree);

  return (
    <li className="group/row relative">
      <button
        type="button"
        data-nav-item
        aria-current={selected ? "true" : undefined}
        onClick={onSelect}
        className={cn(
          "flex h-7 w-full items-center gap-2 rounded-md pr-2 pl-7 text-left text-ui",
          selected
            ? "bg-sidebar-accent text-sidebar-accent-foreground"
            : "text-muted-foreground hover:bg-sidebar-accent/50 hover:text-sidebar-foreground",
        )}
      >
        <StateDot state={runningCount > 0 ? "running" : "idle"} />
        <span
          className={cn("min-w-0 flex-1 truncate", label.detached && "italic")}
          title={label.text}
        >
          {label.text}
        </span>
        <span className="sr-only">
          {runningCount > 0
            ? copy.worktree.runningTerminals(runningCount)
            : copy.worktree.noTerminals}
        </span>
        {worktree.prunable && (
          <TriangleAlert
            className="size-3 shrink-0 text-destructive"
            aria-label={copy.worktree.missingDirectory}
          />
        )}
        {worktree.locked && (
          <Lock className="size-3 shrink-0" aria-label={copy.worktree.locked} />
        )}
        {worktree.isMain && (
          <span className="shrink-0 rounded-sm px-1 text-meta text-muted-foreground ring-1 ring-border ring-inset group-focus-within/row:invisible group-hover/row:invisible">
            {copy.worktree.primary}
          </span>
        )}
      </button>

      {selected && worktree.baseRef && (
        <p className="truncate pr-2 pb-1 pl-[3.25rem] text-meta text-muted-foreground">
          {copy.worktree.from(worktree.baseRef)}
        </p>
      )}

      <div
        className={cn(
          "absolute top-0.5 right-1 flex items-center gap-0.5 rounded-md bg-sidebar-accent opacity-0 transition-opacity",
          "group-focus-within/row:opacity-100 group-hover/row:opacity-100",
        )}
      >
        <NewTerminalMenu worktreePath={worktree.path}>
          <TooltipIconButton
            label={copy.worktree.newTerminal}
            shortcut={selected ? shortcutLabel("newTerminal") : undefined}
          >
            <SquareTerminal />
          </TooltipIconButton>
        </NewTerminalMenu>
        {!worktree.isMain && (
          <DropdownMenu>
            <DropdownMenuTrigger asChild>
              <TooltipIconButton label={copy.worktree.actions(label.text)}>
                <MoreHorizontal />
              </TooltipIconButton>
            </DropdownMenuTrigger>
            <DropdownMenuContent align="end" className="min-w-44">
              <DropdownMenuItem
                variant="destructive"
                onSelect={() => askRemoveWorktree({ repoId, worktree })}
              >
                {copy.worktree.remove}
              </DropdownMenuItem>
            </DropdownMenuContent>
          </DropdownMenu>
        )}
      </div>
    </li>
  );
}
