import { describe, expect, it } from "vitest";

import { MAX_AGENT_BADGES, splitAgentBadges } from "@/agents/agent-badges";

describe("splitAgentBadges", () => {
  it("shows every agent when there are few, and hides none", () => {
    expect(splitAgentBadges(["a"])).toEqual({ shown: ["a"], hidden: 0 });
    expect(splitAgentBadges(["a", "b", "c"])).toEqual({
      shown: ["a", "b", "c"],
      hidden: 0,
    });
  });

  it("shows the first three and counts the rest", () => {
    expect(MAX_AGENT_BADGES).toBe(3);
    expect(splitAgentBadges(["a", "b", "c", "d"])).toEqual({
      shown: ["a", "b", "c"],
      hidden: 1,
    });
    expect(splitAgentBadges(["a", "b", "c", "d", "e", "f"])).toEqual({
      shown: ["a", "b", "c"],
      hidden: 3,
    });
  });

  it("is empty for no agents", () => {
    expect(splitAgentBadges([])).toEqual({ shown: [], hidden: 0 });
  });
});
