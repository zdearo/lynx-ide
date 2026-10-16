import { CircleCheck, CircleDot, CircleX, Sparkles, X } from "lucide-react";

import { TooltipIconButton } from "@/components/TooltipIconButton";
import { cn } from "@/lib/utils";
import { formatAge } from "@/terminal/relative-time";
import { copy } from "@/ui-copy";

export interface AgentItem {
  id: string;
  title: string;
  running: boolean;
  exitCode: number | null;
  startedAt: number;
}

/** Running is the only live state we can tell apart today; working/idle/needs-you arrive with state detection. */
export function AgentStatusIcon({ agent }: { agent: AgentItem }) {
  if (agent.running) {
    return <CircleDot aria-hidden className="size-3 shrink-0 text-live" />;
  }
  if (agent.exitCode === 0) {
    return (
      <CircleCheck
        aria-hidden
        className="size-3 shrink-0 text-muted-foreground"
      />
    );
  }
  return <CircleX aria-hidden className="size-3 shrink-0 text-destructive" />;
}

interface AgentRowProps {
  agent: AgentItem;
  /** Its tab is the one showing in the terminal area. */
  active: boolean;
  now: number;
  onOpen: () => void;
  onClose: () => void;
}

/** One agent session under its worktree: status, glyph, title and how long ago it started. */
export function AgentRow({
  agent,
  active,
  now,
  onOpen,
  onClose,
}: AgentRowProps) {
  const status = agent.running
    ? copy.agent.running
    : copy.agent.exited(agent.exitCode);

  return (
    <li className="group/agent relative">
      <button
        type="button"
        data-nav-item
        data-agent-id={agent.id}
        aria-current={active ? "true" : undefined}
        aria-label={copy.agent.label(agent.title, status)}
        onClick={onOpen}
        // No surface unless it is the open agent: one lighter pill on the card is the
        // only emphasis.
        className={cn(
          "flex h-7 w-full items-center gap-1.5 rounded-md pr-2 pl-2 text-left text-nested",
          active
            ? "bg-foreground/10 text-sidebar-foreground"
            : "text-muted-foreground hover:bg-foreground/5 hover:text-sidebar-foreground",
        )}
      >
        <AgentStatusIcon agent={agent} />
        <Sparkles aria-hidden className="size-3 shrink-0 text-agent" />
        <span className="min-w-0 flex-1 truncate">{agent.title}</span>
        <span
          aria-hidden
          className="shrink-0 text-meta text-muted-foreground tabular-nums group-focus-within/agent:invisible group-hover/agent:invisible"
        >
          {formatAge(agent.startedAt, now)}
        </span>
      </button>
      <div
        className={cn(
          "absolute top-1 right-1 opacity-0 transition-opacity",
          "group-focus-within/agent:opacity-100 group-hover/agent:opacity-100",
        )}
      >
        <TooltipIconButton
          label={copy.agent.close(agent.title)}
          tabIndex={-1}
          className="size-5"
          onClick={onClose}
        >
          <X />
        </TooltipIconButton>
      </div>
    </li>
  );
}
