import { describe, expect, it } from "vitest";

import type { RepoRefs } from "@/bindings";
import {
  BASE_REF_LIMIT,
  baseRefOptions,
  searchBaseRefs,
} from "@/workspace/worktrees/base-ref-search";

const repo: RepoRefs = {
  defaultBase: "origin/main",
  refs: ["feature/login", "main", "origin/main", "origin/release", "fix/pty"],
};

const many = Array.from(
  { length: 250 },
  (_, index) => `feature/topic-${index}`,
);

describe("baseRefOptions", () => {
  it("puts the default first and lists every other ref once", () => {
    expect(baseRefOptions(repo)).toEqual([
      "origin/main",
      "feature/login",
      "main",
      "origin/release",
      "fix/pty",
    ]);
  });

  it("is empty until the refs load", () => {
    expect(baseRefOptions(null)).toEqual([]);
  });
});

describe("searchBaseRefs", () => {
  it("returns the first rows and the full count when the query is empty", () => {
    const result = searchBaseRefs(many, "");

    expect(result.shown).toHaveLength(BASE_REF_LIMIT);
    expect(result.shown[0]).toBe("feature/topic-0");
    expect(result.total).toBe(250);
  });

  it("treats a blank query as no query", () => {
    expect(searchBaseRefs(["a", "b"], "   ")).toEqual({
      shown: ["a", "b"],
      total: 2,
    });
  });

  it("matches case-insensitively as a substring", () => {
    const result = searchBaseRefs(
      ["origin/Feature/Login", "main", "fix/pty"],
      " FEAT ",
    );

    expect(result).toEqual({ shown: ["origin/Feature/Login"], total: 1 });
  });

  it("caps the rows and still reports every match", () => {
    const result = searchBaseRefs(many, "topic-1");

    expect(result.shown).toHaveLength(BASE_REF_LIMIT);
    expect(result.total).toBe(111);
  });

  it("returns nothing when no ref matches", () => {
    expect(searchBaseRefs(many, "nope")).toEqual({ shown: [], total: 0 });
  });
});
