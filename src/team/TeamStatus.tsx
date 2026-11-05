import { CircleCheckGlyph18 } from "@/components/icons/CircleCheckGlyph18";
import { CircleQuestionGlyph18 } from "@/components/icons/CircleQuestionGlyph18";
import { CircleXmarkGlyph18 } from "@/components/icons/CircleXmarkGlyph18";
import { Loader2Glyph18 } from "@/components/icons/Loader2Glyph18";
import { RadioCheckedGlyph18 } from "@/components/icons/RadioCheckedGlyph18";
import type { TeamMember, TeamStatus } from "@/team/team-fixtures";
import { teamCounts } from "@/team/team-counts";
import { useTerminalStore } from "@/terminal/terminal-store";
import { copy } from "@/ui-copy";

export function TeamStatusIcon({ status }: { status: TeamStatus }) {
  switch (status) {
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
    case "closed":
      return (
        <CircleCheckGlyph18
          aria-hidden
          className="size-3 shrink-0 text-muted-foreground"
        />
      );
    case "failed":
      return (
        <CircleXmarkGlyph18
          aria-hidden
          className="size-3 shrink-0 text-destructive"
        />
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
