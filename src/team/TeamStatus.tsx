import type { AgentStatus } from "@/bindings";
import { CircleCheckGlyph18 } from "@/components/icons/CircleCheckGlyph18";
import { CircleQuestionGlyph18 } from "@/components/icons/CircleQuestionGlyph18";
import { Loader2Glyph18 } from "@/components/icons/Loader2Glyph18";
import { RadioCheckedGlyph18 } from "@/components/icons/RadioCheckedGlyph18";
import type { TeamMember } from "@/team/team-fixtures";
import { teamCounts } from "@/team/team-counts";
import { copy } from "@/ui-copy";

export function TeamStatusIcon({ status }: { status: AgentStatus }) {
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
      return (
        <CircleCheckGlyph18
          aria-hidden
          className="size-3 shrink-0 text-muted-foreground"
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

/** "2 working · 1 need you", for the top bar and the side panel. */
export function TeamSummary({ members }: { members: readonly TeamMember[] }) {
  const { working, needsYou } = teamCounts(members);
  const parts = [
    working > 0 ? copy.team.summary.working(working) : null,
    needsYou > 0 ? copy.team.summary.needsYou(needsYou) : null,
  ].filter((part): part is string => part !== null);
  return (
    <span className="text-meta text-muted-foreground tabular-nums">
      {parts.length > 0 ? parts.join(" · ") : copy.team.summary.quiet}
    </span>
  );
}
