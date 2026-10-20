import { describe, expect, it } from "vitest";

import type { PlanUsage, PlanWindow } from "@/bindings";
import {
  agoText,
  barFraction,
  fullestWindow,
  paceOf,
  paceText,
  planText,
  resetText,
  secondsUntilReset,
  shortDuration,
  shownLabel,
  shownPercent,
  summaryOf,
  tickPosition,
  toneOf,
  windowName,
  windowShort,
} from "@/usage/plan-usage-view";

const levels = { warnAt: 75, criticalAt: 90 };
const used = { display: "used" as const };
const left = { display: "remaining" as const };

const window = (
  kind: PlanWindow["kind"],
  usedPercent: number,
  resetsAt: number | null = null,
  minutes: number | null = null,
): PlanWindow => ({ kind, minutes, usedPercent, resetsAt });

const usage = (overrides: Partial<PlanUsage>): PlanUsage => ({
  harness: "codex",
  plan: "plus",
  state: "ready",
  reason: null,
  windows: [],
  observedAt: 0,
  ...overrides,
});

describe("toneOf", () => {
  it("is calm below the warning level, close from it, and full from the critical one", () => {
    expect(toneOf(0, levels)).toBe("calm");
    expect(toneOf(74.9, levels)).toBe("calm");
    expect(toneOf(75, levels)).toBe("close");
    expect(toneOf(89.9, levels)).toBe("close");
    expect(toneOf(90, levels)).toBe("full");
    expect(toneOf(140, levels)).toBe("full");
  });

  it("follows the levels the user set, not the defaults", () => {
    const strict = { warnAt: 50, criticalAt: 60 };
    expect(toneOf(55, strict)).toBe("close");
    expect(toneOf(60, strict)).toBe("full");
  });
});

describe("shown percentages", () => {
  it("reads used or what is left, and never below zero", () => {
    expect(shownPercent(17.4, "used")).toBe(17);
    expect(shownPercent(17.4, "remaining")).toBe(83);
    expect(shownPercent(120, "remaining")).toBe(0);
    expect(shownLabel(17, "used")).toBe("17% used");
    expect(shownLabel(17, "remaining")).toBe("83% left");
  });

  it("draws the bar at the number shown, so a bar of what is left drains", () => {
    expect(barFraction(25, "used")).toBe(0.25);
    expect(barFraction(25, "remaining")).toBe(0.75);
    expect(barFraction(150, "used")).toBe(1);
  });

  it("puts a level on the bar where that number falls", () => {
    expect(tickPosition(75, "used")).toBe(75);
    expect(tickPosition(75, "remaining")).toBe(25);
  });
});

describe("fullestWindow and the reset countdown", () => {
  it("picks the window nearest its limit, whatever its position", () => {
    const fullest = fullestWindow([
      window("session", 20),
      window("weekly", 88),
      window("monthly", 5),
    ]);
    expect(fullest?.kind).toBe("weekly");
    expect(fullestWindow([])).toBeNull();
  });

  it("counts down to the reset and never goes negative", () => {
    expect(secondsUntilReset(window("session", 1, 1_000), 400)).toBe(600);
    expect(secondsUntilReset(window("session", 1, 1_000), 2_000)).toBe(0);
    expect(secondsUntilReset(window("session", 1), 400)).toBeNull();
  });
});

describe("durations and reset text", () => {
  it("speaks in the largest units that fit", () => {
    expect(shortDuration(45 * 60)).toBe("45m");
    expect(shortDuration(3 * 3600 + 10 * 60)).toBe("3h 10m");
    expect(shortDuration(50 * 3600)).toBe("2d 2h");
    expect(shortDuration(20 * 86400)).toBe("20d");
  });

  it("says when a window resets, or that it is not known", () => {
    expect(resetText(null)).toBe("Reset time unknown");
    expect(resetText(30)).toBe("Resets now");
    expect(resetText(5 * 60)).toBe("Resets in 5m");
    expect(resetText(3 * 3600 + 5 * 60)).toBe("Resets in 3h 5m");
  });
});

describe("window names", () => {
  it("names each window by its kind, and says how long an unnamed one lasts", () => {
    expect(windowName(window("session", 0))).toBe("5-hour window");
    expect(windowName(window("weekly", 0))).toBe("Weekly window");
    expect(windowName(window("monthly", 0))).toBe("Monthly window");
    expect(windowName(window("other", 0, null, 90 * 24 * 60))).toBe(
      "90-day window",
    );
    expect(windowName(window("other", 0))).toBe("Other window");
  });

  it("has a short name for the footer", () => {
    expect(windowShort(window("session", 0))).toBe("5h");
    expect(windowShort(window("weekly", 0))).toBe("wk");
    expect(windowShort(window("monthly", 0))).toBe("mo");
  });
});

describe("pace", () => {
  const start = 1_000_000;
  const fiveHours = 5 * 3600;

  it("says when a window would be full at the rate used so far", () => {
    // 20% used after one hour of a five-hour window: 80% more at 20% an hour is four hours away,
    // which is after the reset in four hours, so it just holds. Use 40% for a clear run-out.
    const now = start + 3600;
    const resetsAt = start + fiveHours;
    const pace = paceOf(window("session", 40, resetsAt, 300), now);

    expect(pace?.kind).toBe("runsOut");
    if (pace?.kind === "runsOut") {
      // 60% left at 40% an hour: an hour and a half.
      expect(pace.seconds).toBeCloseTo(1.5 * 3600, 0);
    }
  });

  it("says the window lasts until the reset when it would not fill in time", () => {
    const now = start + 3600;
    const resetsAt = start + fiveHours;
    expect(paceOf(window("session", 10, resetsAt, 300), now)).toEqual({
      kind: "holds",
    });
  });

  it("stays silent when there is too little to go on", () => {
    const resetsAt = start + fiveHours;
    expect(
      paceOf(window("session", 0, resetsAt, 300), start + 3600),
    ).toBeNull();
    expect(paceOf(window("session", 5, resetsAt, 300), start + 60)).toBeNull();
    expect(
      paceOf(window("session", 100, resetsAt, 300), start + 3600),
    ).toBeNull();
    expect(paceOf(window("session", 5, null, 300), start + 3600)).toBeNull();
    expect(paceOf(window("session", 5, start, 300), start + 3600)).toBeNull();
  });

  it("is worded for the footer and the popover", () => {
    expect(paceText({ kind: "runsOut", seconds: 3 * 3600 })).toBe(
      "At this pace: full in 3h 0m",
    );
    expect(paceText({ kind: "holds" })).toBe(
      "At this pace: lasts until the reset",
    );
  });
});

describe("summaryOf", () => {
  const now = 1_000;

  it("reads out the fullest window, as used or as left", () => {
    const plan = usage({
      windows: [window("monthly", 12.4, now + 3600, 43_200)],
    });
    expect(summaryOf("Codex", plan, now, { ...levels, ...used })).toBe(
      "Codex: 12% used, Monthly window. Resets in 1h 0m.",
    );
    expect(summaryOf("Codex", plan, now, { ...levels, ...left })).toBe(
      "Codex: 88% left, Monthly window. Resets in 1h 0m.",
    );
  });

  it("says what is missing when the plan is not signed in or failed", () => {
    expect(
      summaryOf("Codex", usage({ state: "signInNeeded" }), now, {
        ...levels,
        ...used,
      }),
    ).toContain("Sign in to Codex");
    expect(
      summaryOf("Codex", usage({ state: "unavailable" }), now, {
        ...levels,
        ...used,
      }),
    ).toContain("unavailable");
  });
});

describe("agoText and planText", () => {
  it("says how old the read is, and capitalizes the plan name for its badge", () => {
    expect(agoText(1_000, 1_030)).toBe("just now");
    expect(agoText(1_000, 1_000 + 3 * 60)).toBe("3 min ago");
    expect(agoText(1_000, 1_000 + 5 * 3600)).toBe("5 h ago");
    expect(planText("pro")).toBe("Pro");
    expect(planText(null)).toBeNull();
  });
});
