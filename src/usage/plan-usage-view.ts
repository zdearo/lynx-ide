import type { HarnessId, PlanUsage, PlanWindow, UsageConfig } from "@/bindings";
import { TERMINAL_PRESETS } from "@/agents/agent-presets";
import { copy } from "@/ui-copy";

export type UsageTone = "calm" | "close" | "full";
/** The two levels a bar warns at, as the settings hold them. */
export type UsageLevels = Pick<UsageConfig, "warnAt" | "criticalAt">;

/** A pace needs a little history: the first minutes of a window say too little about its rate. */
const MIN_ELAPSED_SECONDS = 10 * 60;

export function nameOf(harness: HarnessId): string {
  return (
    TERMINAL_PRESETS.find((preset) => preset.harness === harness)?.label ??
    harness
  );
}

export function toneOf(usedPercent: number, levels: UsageLevels): UsageTone {
  if (usedPercent >= levels.criticalAt) return "full";
  if (usedPercent >= levels.warnAt) return "close";
  return "calm";
}

/** The whole percent as the user chose to read it: used, or what is left. */
export function shownPercent(
  usedPercent: number,
  display: UsageConfig["display"],
): number {
  const used = Math.round(Math.max(0, usedPercent));
  return display === "used" ? used : Math.max(0, 100 - used);
}

export function shownLabel(
  usedPercent: number,
  display: UsageConfig["display"],
): string {
  const percent = shownPercent(usedPercent, display);
  return display === "used"
    ? copy.planLimits.percentUsed(percent)
    : copy.planLimits.percentLeft(percent);
}

/** How full a bar is, from 0 to 1: it tracks the number shown, so a bar of what is left drains as the plan is used. */
export function barFraction(
  usedPercent: number,
  display: UsageConfig["display"],
): number {
  const used = Math.min(1, Math.max(0, usedPercent / 100));
  return display === "used" ? used : 1 - used;
}

/** Where a warning level sits on the bar, in percent of its width. A bar of what is left runs the other way. */
export function tickPosition(
  level: number,
  display: UsageConfig["display"],
): number {
  return display === "used" ? level : 100 - level;
}

/** The window nearest its limit: the one the footer reads for a plan. */
export function fullestWindow(
  windows: readonly PlanWindow[],
): PlanWindow | null {
  let fullest: PlanWindow | null = null;
  for (const window of windows) {
    if (fullest === null || window.usedPercent > fullest.usedPercent) {
      fullest = window;
    }
  }
  return fullest;
}

/** Seconds until the window starts over, or null when the source did not say when. */
export function secondsUntilReset(
  window: PlanWindow,
  nowSeconds: number,
): number | null {
  if (window.resetsAt === null) return null;
  return Math.max(0, window.resetsAt - nowSeconds);
}

/** A span of time in the largest units that fit: "45m", "3h 10m", "2d 4h". */
export function shortDuration(seconds: number): string {
  const minutes = Math.floor(seconds / 60);
  if (minutes < 60) return copy.planLimits.duration.minutes(minutes);
  const hours = Math.floor(minutes / 60);
  if (hours < 48) {
    return copy.planLimits.duration.hours(hours, minutes % 60);
  }
  const days = Math.floor(hours / 24);
  return hours % 24 === 0
    ? copy.planLimits.duration.exactDays(days)
    : copy.planLimits.duration.days(days, hours % 24);
}

export function resetText(seconds: number | null): string {
  if (seconds === null) return copy.planLimits.resets.unknown;
  if (seconds < 60) return copy.planLimits.resets.now;
  return copy.planLimits.resets.in(shortDuration(seconds));
}

export function windowName(window: PlanWindow): string {
  switch (window.kind) {
    case "session":
      return copy.planLimits.window.session;
    case "weekly":
      return copy.planLimits.window.weekly;
    case "monthly":
      return copy.planLimits.window.monthly;
    case "other":
      return copy.planLimits.window.other(window.minutes);
  }
}

/** The short name a footer chip or a compact label can carry: "5h", "wk", "mo". */
export function windowShort(window: PlanWindow): string {
  switch (window.kind) {
    case "session":
      return copy.planLimits.windowShort.session;
    case "weekly":
      return copy.planLimits.windowShort.weekly;
    case "monthly":
      return copy.planLimits.windowShort.monthly;
    case "other":
      return copy.planLimits.windowShort.other(window.minutes);
  }
}

export function toneText(tone: UsageTone): string {
  switch (tone) {
    case "calm":
      return "";
    case "close":
      return copy.planLimits.tone.close;
    case "full":
      return copy.planLimits.tone.full;
  }
}

/** How fast the window is filling, and when it would be full at that rate. */
export type Pace = { kind: "runsOut"; seconds: number } | { kind: "holds" };

/**
 * At the rate used so far, when the window fills. It assumes use grew at a steady rate since the
 * window began, so it is an estimate, and it is silent while there is too little to go on.
 */
export function paceOf(window: PlanWindow, nowSeconds: number): Pace | null {
  if (window.minutes === null || window.resetsAt === null) return null;
  const elapsed = window.minutes * 60 - (window.resetsAt - nowSeconds);
  if (window.resetsAt <= nowSeconds || elapsed < MIN_ELAPSED_SECONDS) {
    return null;
  }
  if (window.usedPercent <= 0 || window.usedPercent >= 100) return null;

  const secondsToFull =
    ((100 - window.usedPercent) * elapsed) / window.usedPercent;
  if (nowSeconds + secondsToFull >= window.resetsAt) return { kind: "holds" };
  return { kind: "runsOut", seconds: secondsToFull };
}

export function paceText(pace: Pace): string {
  return pace.kind === "runsOut"
    ? copy.planLimits.pace.runsOut(shortDuration(pace.seconds))
    : copy.planLimits.pace.holds;
}

/** What a screen reader hears for a plan's footer button: the name, the fullest window, and when it resets. */
export function summaryOf(
  name: string,
  usage: PlanUsage,
  nowSeconds: number,
  config: UsageLevels & Pick<UsageConfig, "display">,
): string {
  if (usage.state === "signInNeeded") {
    return copy.planLimits.signInDetail(name);
  }
  if (usage.state === "unavailable") {
    return copy.planLimits.unavailableSummary(name);
  }
  const window = fullestWindow(usage.windows);
  if (window === null) return name;
  return copy.planLimits.summary(
    name,
    shownLabel(window.usedPercent, config.display),
    windowName(window),
    resetText(secondsUntilReset(window, nowSeconds)),
  );
}

/** How long ago the numbers were read, from unix seconds. */
export function agoText(observedAt: number, nowSeconds: number): string {
  const seconds = Math.max(0, nowSeconds - observedAt);
  if (seconds < 60) return copy.planLimits.ago.now;
  const minutes = Math.floor(seconds / 60);
  if (minutes < 60) return copy.planLimits.ago.minutes(minutes);
  return copy.planLimits.ago.hours(Math.floor(minutes / 60));
}

/** The plan's name as the provider writes it, capitalized for a badge: "Pro", "Free". */
export function planText(plan: string | null): string | null {
  if (plan === null || plan === "") return null;
  return plan.charAt(0).toUpperCase() + plan.slice(1);
}

/** Where the numbers come from, said once so nobody wonders whether a model was asked. */
export function sourceText(harness: HarnessId): string | null {
  if (harness === "claude") return copy.planLimits.source.claude;
  if (harness === "codex") return copy.planLimits.source.codex;
  return null;
}

export function signInHint(harness: HarnessId): string | null {
  if (harness === "claude") return copy.planLimits.signInHint.claude;
  if (harness === "codex") return copy.planLimits.signInHint.codex;
  return null;
}
