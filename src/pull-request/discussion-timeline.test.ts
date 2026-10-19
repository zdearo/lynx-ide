import { describe, expect, it } from "vitest";

import type { DiscussionComment, ReviewSummary } from "@/bindings";
import { isoSeconds, timeline } from "@/pull-request/discussion-timeline";

const comment = (id: string, createdAt: string): DiscussionComment => ({
  id,
  author: "ana",
  body: id,
  createdAt,
  url: "u",
});

const review = (author: string, submittedAt: string): ReviewSummary => ({
  author,
  verdict: "approved",
  body: "",
  submittedAt,
});

describe("timeline", () => {
  it("puts comments and reviews in one list, oldest first", () => {
    const entries = timeline(
      [
        comment("late", "2026-10-19T12:00:00Z"),
        comment("early", "2026-10-19T09:00:00Z"),
      ],
      [review("bruno", "2026-10-19T10:00:00Z")],
    );

    expect(entries.map((entry) => entry.kind)).toEqual([
      "comment",
      "review",
      "comment",
    ]);
    expect(entries[0].kind === "comment" && entries[0].comment.id).toBe(
      "early",
    );
  });

  it("reads ISO dates as seconds and a broken date as zero", () => {
    expect(isoSeconds("2026-10-19T10:00:00Z")).toBe(1_792_404_000);
    expect(isoSeconds("not a date")).toBe(0);
  });
});
