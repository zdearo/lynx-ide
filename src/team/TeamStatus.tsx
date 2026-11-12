import {
  CheckCircleIcon,
  QuestionIcon,
  RadioButtonIcon,
  SpinnerGapIcon,
  XCircleIcon,
} from "@phosphor-icons/react";
import type { TeamMember, TeamStatus } from "@/team/team-fixtures";
import { teamCounts } from "@/team/team-counts";
import { useTerminalStore } from "@/terminal/terminal-store";
import { copy } from "@/ui-copy";

export function TeamStatusIcon({ status }: { status: TeamStatus }) {
  switch (status) {
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
    case "closed":
      return (
        <CheckCircleIcon
          aria-hidden
          className="size-3 shrink-0 text-muted-foreground"
        />
      );
    case "failed":
      return (
        <XCircleIcon aria-hidden className="size-3 shrink-0 text-destructive" />
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

/** "2 working · 1 need you · 1 running", for the top bar and the side panel. */
export function TeamSummary({ members }: { members: readonly TeamMember[] }) {
  const sessions = useTerminalStore((state) => state.sessions);
  const { working, needsYou, running } = teamCounts(members, sessions);
  const parts = [
    working > 0 ? copy.team.summary.working(working) : null,
    needsYou > 0 ? copy.team.summary.needsYou(needsYou) : null,
    running > 0 ? copy.team.summary.running(running) : null,
  ].filter((part): part is string => part !== null);
  return (
    <span className="text-meta text-muted-foreground tabular-nums">
      {parts.length > 0 ? parts.join(" · ") : copy.team.summary.quiet}
    </span>
  );
}
