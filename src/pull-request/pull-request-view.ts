import type { CheckState, PullRequest, PullRequestCheck } from "@/bindings";

/** The one word that says what state a pull request is in. A draft is an open pull request. */
export type PullRequestBadge = "draft" | "open" | "merged" | "closed";

export function badgeOf(pullRequest: PullRequest): PullRequestBadge {
  if (pullRequest.state === "merged") return "merged";
  if (pullRequest.state === "closed") return "closed";
  return pullRequest.isDraft ? "draft" : "open";
}

export interface CheckCounts {
  passed: number;
  failed: number;
  pending: number;
  skipped: number;
}

export function countChecks(checks: readonly PullRequestCheck[]): CheckCounts {
  const counts: CheckCounts = { passed: 0, failed: 0, pending: 0, skipped: 0 };
  for (const check of checks) counts[check.state] += 1;
  return counts;
}

// What needs a look comes first: failing, then still running, then done.
const ORDER: Record<CheckState, number> = {
  failed: 0,
  pending: 1,
  passed: 2,
  skipped: 3,
};

export function sortChecks(
  checks: readonly PullRequestCheck[],
): PullRequestCheck[] {
  return [...checks].sort(
    (a, b) => ORDER[a.state] - ORDER[b.state] || a.name.localeCompare(b.name),
  );
}
