import type { HarnessId } from "@/bindings";
import type { AgentStatus } from "@/bindings";
import { CircleCheckGlyph18 } from "@/components/icons/CircleCheckGlyph18";
import { CircleQuestionGlyph18 } from "@/components/icons/CircleQuestionGlyph18";
import { CircleXmarkGlyph18 } from "@/components/icons/CircleXmarkGlyph18";
import { Loader2Glyph18 } from "@/components/icons/Loader2Glyph18";
import { RadioCheckedGlyph18 } from "@/components/icons/RadioCheckedGlyph18";
import { AgentBrandIcon } from "@/components/icons/brands/AgentBrandIcon";
import { XmarkGlyph18 } from "@/components/icons/XmarkGlyph18";

import { TooltipIconButton } from "@/components/TooltipIconButton";
import { cn } from "@/lib/utils";
import { formatAge } from "@/terminal/relative-time";
import { agentStatusText } from "@/agents/agent-status";
import { copy } from "@/ui-copy";

export interface AgentItem {
  id: string;
  harness: HarnessId;
  title: string;
  running: boolean;
  exitCode: number | null;
  startedAt: number;
  /** Null until the monitor has reported a state. */
  status: AgentStatus | null;
}

/** Each state has its own shape, not just its own color: a spinner works, a question mark waits, a check is done. */
export function AgentStatusIcon({ agent }: { agent: AgentItem }) {
  if (!agent.running) {
    return agent.exitCode === 0 ? (
      <CircleCheckGlyph18
        aria-hidden
        className="size-3 shrink-0 text-muted-foreground"
      />
    ) : (
      <CircleXmarkGlyph18
        aria-hidden
        className="size-3 shrink-0 text-destructive"
      />
    );
  }
  switch (agent.status) {
    case "working":
      return (
        <Loader2Glyph18
          aria-hidden
          className="size-3 shrink-0 animate-spin text-live"
        />
      );
    case "needsYou":
      return (
        <CircleQuestionGlyph18
          aria-hidden
          className="size-3 shrink-0 text-attention"
        />
      );
    case "done":
      return (
        <CircleCheckGlyph18 aria-hidden className="size-3 shrink-0 text-live" />
      );
    default:
      return (
        <RadioCheckedGlyph18
          aria-hidden
          className="size-3 shrink-0 text-muted-foreground"
        />
      );
  }
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
  const status = agentStatusText(agent.running, agent.status, agent.exitCode);

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
        <AgentBrandIcon
          harness={agent.harness}
          className="size-3 shrink-0 text-agent"
        />
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
          <XmarkGlyph18 />
        </TooltipIconButton>
      </div>
    </li>
  );
}
