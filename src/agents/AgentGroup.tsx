import { CaretRightIcon } from "@phosphor-icons/react";
import { AgentBrandIcon } from "@/components/icons/brands/AgentBrandIcon";

import { useNow } from "@/hooks/use-now";
import { cn } from "@/lib/utils";
import { copy } from "@/ui-copy";
import { AgentRow, AgentStatusIcon, type AgentItem } from "@/agents/AgentRow";
import { splitAgentBadges } from "@/agents/agent-badges";

interface AgentGroupProps {
  worktreePath: string;
  agents: AgentItem[];
  activeAgentId: string | null;
  expanded: boolean;
  onToggle: () => void;
  onOpenAgent: (id: string) => void;
  onCloseAgent: (id: string) => void;
}

/**
 * A worktree's agents. Collapsed, each agent is a badge side by side, up to three,
 * then "+X" for the rest. Expanded, the header is just the word "Agents" and the
 * rows follow it.
 */
export function AgentGroup({
  worktreePath,
  agents,
  activeAgentId,
  expanded,
  onToggle,
  onOpenAgent,
  onCloseAgent,
}: AgentGroupProps) {
  const running = agents.filter((agent) => agent.running).length;
  const { shown, hidden } = splitAgentBadges(agents);

  return (
    <div data-agent-group={worktreePath}>
      <button
        type="button"
        aria-expanded={expanded}
        aria-label={copy.agent.group(agents.length, running)}
        onClick={onToggle}
        // No colour of its own: it reads as part of the card, and only hover lights it up.
        className="flex h-7 w-full items-center gap-1.5 rounded-md pr-1.5 pl-2 text-left text-ui hover:bg-foreground/5"
      >
        {expanded ? (
          <span className="min-w-0 flex-1 truncate font-medium text-sidebar-foreground">
            {copy.agent.title}
          </span>
        ) : (
          <>
            {shown.map((agent) => (
              <AgentBadge key={agent.id} agent={agent} />
            ))}
            {hidden > 0 && (
              <span
                aria-hidden
                className="inline-flex h-5 shrink-0 items-center rounded-md bg-foreground/10 px-1.5 text-meta tabular-nums"
              >
                +{hidden}
              </span>
            )}
            <span aria-hidden className="flex-1" />
          </>
        )}
        <CaretRightIcon
          aria-hidden
          className={cn(
            "size-3.5 shrink-0 transition-transform",
            expanded && "rotate-90",
          )}
        />
      </button>
      {expanded && (
        <AgentRows
          agents={agents}
          activeAgentId={activeAgentId}
          onOpenAgent={onOpenAgent}
          onCloseAgent={onCloseAgent}
        />
      )}
    </div>
  );
}

/** One agent in the collapsed group: its status, then its glyph. */
function AgentBadge({ agent }: { agent: AgentItem }) {
  return (
    <span
      aria-hidden
      className="inline-flex h-5 shrink-0 items-center gap-1 rounded-md bg-foreground/10 px-1.5"
    >
      <AgentStatusIcon agent={agent} />
      <AgentBrandIcon
        data-icon="agent"
        harness={agent.harness}
        className="size-3 shrink-0 text-agent"
      />
    </span>
  );
}

/** Its own component so the clock that keeps the ages fresh only runs while the rows are shown. */
function AgentRows({
  agents,
  activeAgentId,
  onOpenAgent,
  onCloseAgent,
}: Pick<
  AgentGroupProps,
  "agents" | "activeAgentId" | "onOpenAgent" | "onCloseAgent"
>) {
  const now = useNow();
  return (
    <ul className="mt-1 grid grid-cols-1 gap-1">
      {agents.map((agent) => (
        <AgentRow
          key={agent.id}
          agent={agent}
          active={agent.id === activeAgentId}
          now={now}
          onOpen={() => onOpenAgent(agent.id)}
          onClose={() => onCloseAgent(agent.id)}
        />
      ))}
    </ul>
  );
}
