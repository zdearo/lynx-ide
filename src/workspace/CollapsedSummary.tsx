import { CircleQuestionGlyph18 } from "@/components/icons/CircleQuestionGlyph18";
import { copy } from "@/ui-copy";
import type { GroupSummary } from "@/workspace/collapsed-summary";

/**
 * What a collapsed row hides, drawn at its right edge. The visible part is short
 * and hidden from screen readers; the full sentence is `id`, for aria-describedby.
 */
export function CollapsedSummary({
  id,
  kind,
  summary,
}: {
  id: string;
  kind: "folder" | "repo";
  summary: GroupSummary;
}) {
  const shown =
    kind === "folder"
      ? summary.projects > 0
        ? copy.summary.projects(summary.projects)
        : null
      : copy.summary.worktrees(summary.worktrees);

  const spoken = [
    kind === "folder" && summary.projects > 0
      ? copy.summary.projects(summary.projects)
      : null,
    kind === "repo" ? copy.summary.worktrees(summary.worktrees) : null,
    summary.runningAgents > 0
      ? copy.summary.running(summary.runningAgents)
      : null,
    summary.needingYou > 0 ? copy.summary.needsYou(summary.needingYou) : null,
  ].filter((part): part is string => part !== null);

  return (
    <>
      <span
        aria-hidden
        className="flex shrink-0 items-center gap-2 pl-2 text-meta text-muted-foreground tabular-nums"
      >
        {shown && <span>{shown}</span>}
        {summary.runningAgents > 0 && (
          <span className="flex items-center gap-1 text-live">
            <span className="size-1.5 rounded-full bg-live" />
            {summary.runningAgents}
          </span>
        )}
        {summary.needingYou > 0 && (
          <span className="flex items-center gap-1 text-attention">
            <CircleQuestionGlyph18 aria-hidden className="size-3" />
            {summary.needingYou}
          </span>
        )}
      </span>
      <span id={id} className="sr-only">
        {spoken.length > 0 ? spoken.join(", ") : copy.summary.empty}
      </span>
    </>
  );
}
