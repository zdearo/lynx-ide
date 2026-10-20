import type { WorktreeInfo } from "@/bindings";
import { cn } from "@/lib/utils";
import type { AgentItem } from "@/workspace/AgentRow";
import { AgentGroup } from "@/workspace/AgentGroup";
import { WorktreeRow } from "@/workspace/WorktreeRow";

interface WorktreeGroupProps {
  repoId: string;
  worktree: WorktreeInfo;
  selected: boolean;
  runningCount: number;
  /** Agent sessions of this worktree. They stay listed whichever worktree is selected. */
  agents: AgentItem[];
  activeAgentId: string | null;
  agentsExpanded: boolean;
  onSelect: () => void;
  onToggleAgents: () => void;
  onOpenAgent: (id: string) => void;
  onCloseAgent: (id: string) => void;
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
  activeAgentId,
  agentsExpanded,
  onSelect,
  onToggleAgents,
  onOpenAgent,
  onCloseAgent,
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
        "rounded-lg",
        selected
          ? "bg-sidebar-accent"
          : "cursor-pointer hover:bg-sidebar-accent/50",
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
          <AgentGroup
            worktreePath={worktree.path}
            agents={agents}
            activeAgentId={activeAgentId}
            expanded={agentsExpanded}
            onToggle={onToggleAgents}
            onOpenAgent={onOpenAgent}
            onCloseAgent={onCloseAgent}
          />
        </div>
      )}
    </li>
  );
}
