import { describe, expect, it } from "vitest";

import type { PullRequest, PullRequestCheck } from "@/bindings";
import {
  badgeOf,
  countChecks,
  hasJobLog,
  sortChecks,
} from "@/pull-request/pull-request-view";

const pullRequest = (
  state: PullRequest["state"],
  isDraft: boolean,
): PullRequest => ({
  number: 1,
  title: "t",
  url: "https://github.com/acme/app/pull/1",
  state,
  isDraft,
  review: null,
  headBranch: "feat/x",
  baseBranch: "main",
  mergeState: "clean",
  autoMerge: false,
  checks: [],
});

const check = (
  name: string,
  state: PullRequestCheck["state"],
): PullRequestCheck => ({
  name,
  state,
  detailsUrl: null,
});

describe("badgeOf", () => {
  it("reads a draft as its own state, not as open", () => {
    expect(badgeOf(pullRequest("open", true))).toBe("draft");
    expect(badgeOf(pullRequest("open", false))).toBe("open");
    expect(badgeOf(pullRequest("merged", false))).toBe("merged");
    expect(badgeOf(pullRequest("closed", true))).toBe("closed");
  });
});

describe("countChecks", () => {
  it("counts each state and starts from zero", () => {
    expect(
      countChecks([
        check("a", "passed"),
        check("b", "failed"),
        check("c", "pending"),
        check("d", "passed"),
      ]),
    ).toEqual({ passed: 2, failed: 1, pending: 1, skipped: 0 });
    expect(countChecks([])).toEqual({
      passed: 0,
      failed: 0,
      pending: 0,
      skipped: 0,
    });
  });
});

describe("sortChecks", () => {
  it("puts failing checks first, then pending, passed and skipped, by name within each", () => {
    const sorted = sortChecks([
      check("z-pass", "passed"),
      check("skip", "skipped"),
      check("b-fail", "failed"),
      check("run", "pending"),
      check("a-fail", "failed"),
    ]).map((item) => item.name);

    expect(sorted).toEqual(["a-fail", "b-fail", "run", "z-pass", "skip"]);
  });

  it("leaves the list it was given untouched", () => {
    const original = [check("b", "passed"), check("a", "failed")];
    sortChecks(original);
    expect(original.map((item) => item.name)).toEqual(["b", "a"]);
  });
});

describe("hasJobLog", () => {
  it("is true only for the page of a GitHub Actions job", () => {
    const withUrl = (detailsUrl: string | null): PullRequestCheck => ({
      name: "n",
      state: "failed",
      detailsUrl,
    });

    expect(
      hasJobLog(withUrl("https://github.com/a/b/actions/runs/1/job/2")),
    ).toBe(true);
    expect(hasJobLog(withUrl("https://github.com/a/b/actions/runs/1"))).toBe(
      false,
    );
    expect(hasJobLog(withUrl("https://ci.example.com/build/9"))).toBe(false);
    expect(hasJobLog(withUrl(null))).toBe(false);
  });
});
