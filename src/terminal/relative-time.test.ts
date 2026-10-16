import { describe, expect, it } from "vitest";

import { formatAge } from "@/terminal/relative-time";

const NOW_S = 1_800_000_000;
const age = (secondsAgo: number) => formatAge(NOW_S - secondsAgo, NOW_S * 1000);

describe("formatAge", () => {
  it("says now for the first minute", () => {
    expect(age(0)).toBe("now");
    expect(age(59)).toBe("now");
  });

  it("counts whole minutes, hours and days, rounding down", () => {
    expect(age(60)).toBe("1m");
    expect(age(59 * 60 + 59)).toBe("59m");
    expect(age(3600)).toBe("1h");
    expect(age(23 * 3600 + 3599)).toBe("23h");
    expect(age(86400)).toBe("1d");
    expect(age(10 * 86400 + 5)).toBe("10d");
  });

  it("never goes negative when the clocks disagree a little", () => {
    expect(formatAge(NOW_S + 30, NOW_S * 1000)).toBe("now");
  });
});
