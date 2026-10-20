import { describe, expect, it } from "vitest";

import {
  criticalAtBounds,
  providersWith,
  warnAtBounds,
} from "@/settings/usage-settings";

describe("providersWith", () => {
  it("adds a plan in the footer's order, whatever order it was added in", () => {
    expect(providersWith(["codex"], "claude", true)).toEqual([
      "claude",
      "codex",
    ]);
    expect(providersWith([], "codex", true)).toEqual(["codex"]);
  });

  it("removes a plan, and keeps the others in order", () => {
    expect(providersWith(["claude", "codex"], "claude", false)).toEqual([
      "codex",
    ]);
    expect(providersWith(["codex"], "codex", false)).toEqual([]);
  });

  it("does not repeat a plan that is already on", () => {
    expect(providersWith(["claude"], "claude", true)).toEqual(["claude"]);
  });
});

describe("level bounds", () => {
  it("keeps the warning below the critical level", () => {
    expect(warnAtBounds(90)).toEqual({ min: 1, max: 89 });
  });

  it("keeps the critical level above the warning, and within 100", () => {
    expect(criticalAtBounds(75)).toEqual({ min: 76, max: 100 });
  });
});
