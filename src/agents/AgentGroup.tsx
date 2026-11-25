import type { AgentStatus, HarnessId } from "@/bindings";
import {
  CheckCircleIcon,
  QuestionIcon,
  RadioButtonIcon,
  SpinnerGapIcon,
  XCircleIcon,
} from "@phosphor-icons/react";
import { CaretRightIcon } from "@/components/icons/line-icons";
import { AgentBrandIcon } from "@/components/icons/brands/AgentBrandIcon";
import { splitAgentBadges } from "@/agents/agent-badges";
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
      <CheckCircleIcon
        aria-hidden
        className="size-3 shrink-0 text-muted-foreground"
      />
    ) : (
      <XCircleIcon aria-hidden className="size-3 shrink-0 text-destructive" />
    );
  }
  switch (agent.status) {
    case "working":
      return (
        <SpinnerGapIcon
          aria-hidden
          className="size-3 shrink-0 animate-spin text-live"
        />
      );
    case "needsYou":
      return (
        <QuestionIcon aria-hidden className="size-3 shrink-0 text-attention" />
      );
    case "done":
      return (
        <CheckCircleIcon aria-hidden className="size-3 shrink-0 text-live" />
      );
    default:
      return (
        <RadioButtonIcon
          weight="fill"
          aria-hidden
          className="size-3 shrink-0 text-muted-foreground"
        />
      );
  }
}

/**
 * A worktree's agents, always closed: each agent is a badge side by side, up to three, then "+X" for the rest.
 * The agents themselves are in their terminal tabs.
 */
export function AgentGroup({
  worktreePath,
  agents,
}: {
  worktreePath: string;
  agents: AgentItem[];
}) {
  const running = agents.filter((agent) => agent.running).length;
  const { shown, hidden } = splitAgentBadges(agents);

  return (
    <div
      role="group"
      data-agent-group={worktreePath}
      aria-label={copy.agent.group(agents.length, running)}
      className="flex h-7 items-center gap-1.5 rounded-md pr-1.5 pl-2 text-ui"
    >
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
      <CaretRightIcon aria-hidden className="size-3.5 shrink-0" />
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
