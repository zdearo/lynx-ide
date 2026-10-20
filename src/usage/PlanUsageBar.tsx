import { cn } from "@/lib/utils";
import { barFillClass } from "@/usage/plan-usage-colors";
import type { UsageTone } from "@/usage/plan-usage-view";

/** A thin bar for the footer. The text beside it carries the number, so the bar is decoration. */
export function UsageBar({
  fraction,
  tone,
  className,
}: {
  fraction: number;
  tone: UsageTone;
  className?: string;
}) {
  return (
    <span
      aria-hidden
      className={cn(
        "block h-1 shrink-0 overflow-hidden rounded-full bg-muted",
        className,
      )}
    >
      <span
        className={cn("block h-full", barFillClass(tone))}
        style={{ width: `${Math.min(1, Math.max(0, fraction)) * 100}%` }}
      />
    </span>
  );
}
