import { describe, expect, it } from "vitest";

import { threadAge } from "@/bots/thread-age";

describe("threadAge", () => {
  const now = 1_000_000;

  it("says now for the first minute, then counts minutes, hours and days", () => {
    expect(threadAge(now - 5, now)).toBe("now");
    expect(threadAge(now - 60, now)).toBe("1m");
    expect(threadAge(now - 59 * 60, now)).toBe("59m");
    expect(threadAge(now - 3 * 3600, now)).toBe("3h");
    expect(threadAge(now - 2 * 86_400 - 100, now)).toBe("2d");
  });

  it("never goes negative when the clock of a thread is ahead", () => {
    expect(threadAge(now + 500, now)).toBe("now");
  });
});
