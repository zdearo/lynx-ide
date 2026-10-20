import type { PlanUsage, UsageConfig } from "@/bindings";
import { AgentBrandIcon } from "@/components/icons/brands/AgentBrandIcon";
import {
  Popover,
  PopoverContent,
  PopoverTrigger,
} from "@/components/ui/popover";
import { cn } from "@/lib/utils";
import { PlanUsageDetails } from "@/usage/PlanUsageDetails";
import { UsageBar } from "@/usage/PlanUsageBar";
import { percentTextClass } from "@/usage/plan-usage-colors";
import {
  barFraction,
  fullestWindow,
  nameOf,
  secondsUntilReset,
  shortDuration,
  shownLabel,
  summaryOf,
  toneOf,
  windowShort,
  type UsageTone,
} from "@/usage/plan-usage-view";
import { useNowSeconds } from "@/usage/use-now-seconds";
import { copy } from "@/ui-copy";

/** A footer button for one plan. Compact shows the fullest window; detailed shows each window with its bar. */
export function PlanUsageSegment({
  usage,
  config,
}: {
  usage: PlanUsage;
  config: UsageConfig;
}) {
  const now = useNowSeconds();
  const name = nameOf(usage.harness);

  return (
    <Popover>
      <PopoverTrigger asChild>
        <button
          type="button"
          aria-label={summaryOf(name, usage, now, config)}
          className="flex h-5 min-w-0 items-center gap-1.5 rounded-md px-1.5 outline-none hover:bg-foreground/12 focus-visible:ring-2 focus-visible:ring-ring/60 data-[state=open]:bg-foreground/12"
        >
          <AgentBrandIcon harness={usage.harness} className="size-3 shrink-0" />
          {config.showNames && <span className="truncate">{name}</span>}
          <SegmentReading usage={usage} config={config} now={now} />
        </button>
      </PopoverTrigger>
      <PopoverContent
        align="start"
        className="w-80 p-3 text-ui"
        // Focus the panel, not its refresh button: that button's tooltip would open over the first row,
        // and the first Escape would close the tooltip instead of the popover.
        onOpenAutoFocus={(event) => {
          event.preventDefault();
          (event.target as HTMLElement).focus();
        }}
      >
        <PlanUsageDetails usage={usage} name={name} config={config} now={now} />
      </PopoverContent>
    </Popover>
  );
}

function SegmentReading({
  usage,
  config,
  now,
}: {
  usage: PlanUsage;
  config: UsageConfig;
  now: number;
}) {
  if (usage.state !== "ready") {
    return (
      <span className="text-muted-foreground">
        {usage.state === "signInNeeded"
          ? copy.planLimits.signIn
          : copy.planLimits.unavailable}
      </span>
    );
  }
  if (config.layout === "detailed") {
    return (
      <>
        {usage.windows.map((window) => {
          const tone = toneOf(window.usedPercent, config);
          return (
            <span
              key={`${window.kind}-${window.minutes ?? "?"}`}
              className="flex items-center gap-1"
            >
              <span className="text-muted-foreground">
                {windowShort(window)}
              </span>
              <UsageBar
                className="w-8"
                fraction={barFraction(window.usedPercent, config.display)}
                tone={tone}
              />
              <span className={cn("tabular-nums", percentTextClass(tone))}>
                {shownLabel(window.usedPercent, config.display)}
              </span>
            </span>
          );
        })}
      </>
    );
  }

  const window = fullestWindow(usage.windows);
  if (window === null) return null;
  const tone: UsageTone = toneOf(window.usedPercent, config);
  const seconds = secondsUntilReset(window, now);
  return (
    <>
      <UsageBar
        className="w-10"
        fraction={barFraction(window.usedPercent, config.display)}
        tone={tone}
      />
      <span className={cn("tabular-nums", percentTextClass(tone))}>
        {shownLabel(window.usedPercent, config.display)}
      </span>
      {seconds !== null && seconds >= 60 && (
        <span className="tabular-nums text-muted-foreground">
          · {shortDuration(seconds)}
        </span>
      )}
    </>
  );
}
