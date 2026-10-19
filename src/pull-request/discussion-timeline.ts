import type { DiscussionComment, ReviewSummary } from "@/bindings";

/** One entry of the conversation: a comment, or a review that says something. */
export type TimelineEntry =
  | { kind: "comment"; at: number; comment: DiscussionComment }
  | { kind: "review"; at: number; review: ReviewSummary };

/** Seconds since 1970 for GitHub's ISO date, 0 when it cannot be read. */
export function isoSeconds(iso: string): number {
  const ms = Date.parse(iso);
  return Number.isNaN(ms) ? 0 : Math.floor(ms / 1000);
}

/** Comments and reviews together, oldest first, as the conversation reads on GitHub. */
export function timeline(
  comments: readonly DiscussionComment[],
  reviews: readonly ReviewSummary[],
): TimelineEntry[] {
  const entries: TimelineEntry[] = [
    ...comments.map((comment): TimelineEntry => ({
      kind: "comment",
      at: isoSeconds(comment.createdAt),
      comment,
    })),
    ...reviews.map((review): TimelineEntry => ({
      kind: "review",
      at: isoSeconds(review.submittedAt),
      review,
    })),
  ];
  return entries.sort((a, b) => a.at - b.at);
}
