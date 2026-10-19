import { describe, expect, it } from "vitest";

import { relativeTime } from "@/changes/commit-time";

const NOW = 1_700_000_000_000;
const ago = (seconds: number) => NOW / 1000 - seconds;

describe("relativeTime", () => {
  it("counts up from just now to minutes, hours and days", () => {
    expect(relativeTime(ago(10), NOW)).toBe("just now");
    expect(relativeTime(ago(5 * 60), NOW)).toBe("5 min ago");
    expect(relativeTime(ago(3 * 3600), NOW)).toBe("3 h ago");
    expect(relativeTime(ago(2 * 86400), NOW)).toBe("2 d ago");
  });

  it("falls back to the date after a month, and never reads the future as negative", () => {
    expect(relativeTime(ago(40 * 86400), NOW)).toMatch(/^\d{4}-\d{2}-\d{2}$/);
    expect(relativeTime(ago(-500), NOW)).toBe("just now");
  });
});
