import type { UsageTone } from "@/usage/plan-usage-view";

/** The bar's fill. Colour is never the only signal: the percentage and the words say it too. */
export function barFillClass(tone: UsageTone): string {
  switch (tone) {
    case "calm":
      return "bg-foreground/60";
    case "close":
      return "bg-diff-modified";
    case "full":
      return "bg-destructive";
  }
}

export function percentTextClass(tone: UsageTone): string {
  switch (tone) {
    case "calm":
      return "text-foreground";
    case "close":
      return "text-diff-modified";
    case "full":
      return "text-destructive";
  }
}
