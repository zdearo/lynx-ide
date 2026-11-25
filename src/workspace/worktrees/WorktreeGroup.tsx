import type { WorktreeInfo } from "@/bindings";
import { cn } from "@/lib/utils";
import type { AgentItem } from "@/agents/AgentGroup";
import { AgentGroup } from "@/agents/AgentGroup";
import { ResumableList } from "@/workspace/ResumableList";
import { WorktreeRow } from "@/workspace/worktrees/WorktreeRow";

interface WorktreeGroupProps {
  repoId: string;
  worktree: WorktreeInfo;
  selected: boolean;
  runningCount: number;
  /** Agent sessions of this worktree. They stay listed whichever worktree is selected. */
  agents: AgentItem[];
  onSelect: () => void;
}

/**
 * A worktree and its agents as one block: the card (the selection), the worktree
 * line, and the agents inside it. Only the selected worktree gets a card, so the
 * selection stays one surface. Painting the lines separately, edge to edge, reads
 * as two selections.
 */
export function WorktreeGroup({
  repoId,
  worktree,
  selected,
  runningCount,
  agents,
  onSelect,
}: WorktreeGroupProps) {
  return (
    <li
      // The whole block selects the worktree, and lights up as one card on hover. Real
      // controls inside keep their own click; the worktree button stays the keyboard path.
      onClick={(event) => {
        if (selected || !event.currentTarget.contains(event.target as Node)) {
          return;
        }
        const control = (event.target as Element).closest("button, a, [role]");
        if (control && event.currentTarget.contains(control)) return;
        onSelect();
      }}
      className={cn(
        "rounded-lg border",
        selected
          ? "border-hairline bg-card"
          : "cursor-pointer border-transparent hover:bg-card/50",
      )}
    >
      <WorktreeRow
        repoId={repoId}
        worktree={worktree}
        selected={selected}
        runningCount={runningCount}
        onSelect={onSelect}
      />
      {agents.length > 0 && (
        // The group starts where the worktree's own content starts (under its dot).
        <div className="pr-1.5 pb-1.5 pl-7">
          <AgentGroup worktreePath={worktree.path} agents={agents} />
        </div>
      )}
      <ResumableList worktreePath={worktree.path} />
    </li>
  );
}
