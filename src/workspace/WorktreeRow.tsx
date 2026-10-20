import { DotsGlyph18 } from "@/components/icons/DotsGlyph18";
import { LockGlyph18 } from "@/components/icons/LockGlyph18";
import { SquareTerminalGlyph18 } from "@/components/icons/SquareTerminalGlyph18";
import { TriangleWarningGlyph18 } from "@/components/icons/TriangleWarningGlyph18";

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

/**
 * The worktree's own line. It draws no background, selected or hovered: the
 * surrounding `WorktreeGroup` paints one surface for the line and its agents.
 */
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
    <div className="group/row relative">
      <button
        type="button"
        data-nav-item
        aria-current={selected ? "true" : undefined}
        onClick={onSelect}
        className={cn(
          "flex h-7 w-full items-center gap-2 rounded-md pr-2 pl-7 text-left text-ui",
          selected
            ? "text-sidebar-accent-foreground"
            : "text-muted-foreground hover:text-sidebar-foreground",
        )}
      >
        <StateDot state={runningCount > 0 ? "running" : "idle"} />
        <span
          className={cn(
            "min-w-0 flex-1 truncate",
            label.detached && "italic",
            selected && "font-medium",
          )}
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
          <TriangleWarningGlyph18
            className="size-3 shrink-0 text-destructive"
            aria-label={copy.worktree.missingDirectory}
          />
        )}
        {worktree.locked && (
          <LockGlyph18
            className="size-3 shrink-0"
            aria-label={copy.worktree.locked}
          />
        )}
        {worktree.isMain && (
          <span className="shrink-0 text-meta text-muted-foreground group-focus-within/row:invisible group-hover/row:invisible">
            {copy.worktree.primary}
          </span>
        )}
      </button>

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
            <SquareTerminalGlyph18 />
          </TooltipIconButton>
        </NewTerminalMenu>
        {!worktree.isMain && (
          <DropdownMenu>
            <DropdownMenuTrigger asChild>
              <TooltipIconButton label={copy.worktree.actions(label.text)}>
                <DotsGlyph18 />
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
    </div>
  );
}
