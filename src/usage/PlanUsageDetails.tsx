import type { HarnessId, PlanUsage, PlanWindow, UsageConfig } from "@/bindings";
import { AgentBrandIcon } from "@/components/icons/brands/AgentBrandIcon";
import { ArrowsClockwiseIcon } from "@phosphor-icons/react";
import { TooltipIconButton } from "@/components/TooltipIconButton";
import { Button } from "@/components/ui/button";
import { cn } from "@/lib/utils";
import { usePlanUsageStore } from "@/usage/plan-usage-store";
import { percentTextClass, barFillClass } from "@/usage/plan-usage-colors";
import {
  agoText,
  barFraction,
  paceOf,
  paceText,
  planText,
  resetText,
  secondsUntilReset,
  shownLabel,
  signInHint,
  sourceText,
  tickPosition,
  toneOf,
  toneText,
  windowName,
} from "@/usage/plan-usage-view";
import { copy } from "@/ui-copy";

/** The popover behind a footer button: every window of the plan, with its bar, reset and pace. */
export function PlanUsageDetails({
  usage,
  name,
  config,
  now,
}: {
  usage: PlanUsage;
  name: string;
  config: UsageConfig;
  now: number;
}) {
  const plan = planText(usage.plan);
  const source = sourceText(usage.harness);

  return (
    <div className="grid gap-3">
      <header className="flex items-start gap-2">
        <AgentBrandIcon
          harness={usage.harness}
          className="mt-0.5 size-3.5 shrink-0"
        />
        <div className="min-w-0 flex-1">
          <p className="font-medium">{name}</p>
          <p className="flex flex-wrap items-center gap-1.5 text-meta text-muted-foreground">
            {plan && (
              <span className="rounded-sm border border-border px-1 text-foreground">
                {plan}
              </span>
            )}
            <span>
              {copy.planLimits.readAt(agoText(usage.observedAt, now))}
            </span>
          </p>
        </div>
        <RefreshButton />
      </header>

      {usage.state === "ready" &&
        usage.windows.map((window) => (
          <WindowCard
            key={`${window.kind}-${window.minutes ?? "?"}`}
            window={window}
            config={config}
            now={now}
          />
        ))}
      {usage.state === "signInNeeded" && (
        <SignInNote name={name} harness={usage.harness} />
      )}
      {usage.state === "unavailable" && (
        <UnavailableNote reason={usage.reason} />
      )}

      {source && <p className="text-meta text-muted-foreground">{source}</p>}
    </div>
  );
}

function RefreshButton() {
  return (
    <TooltipIconButton
      label={copy.planLimits.refresh}
      onClick={() => {
        usePlanUsageStore
          .getState()
          .refresh(true)
          .catch(() => undefined);
      }}
    >
      <ArrowsClockwiseIcon />
    </TooltipIconButton>
  );
}

/** One window: what is shown, the bar with the levels marked on it, the reset, and the pace. */
function WindowCard({
  window,
  config,
  now,
}: {
  window: PlanWindow;
  config: UsageConfig;
  now: number;
}) {
  const name = windowName(window);
  const tone = toneOf(window.usedPercent, config);
  const shown = shownLabel(window.usedPercent, config.display);
  const pace = paceOf(window, now);
  const ticks = [config.warnAt, config.criticalAt].map((level) =>
    tickPosition(level, config.display),
  );
  const fraction = barFraction(window.usedPercent, config.display);

  return (
    <section
      aria-label={name}
      className="grid gap-1.5 border-t border-border pt-3 first:border-t-0 first:pt-0"
    >
      <div className="flex items-baseline justify-between gap-2">
        <p className="text-nested font-medium">{name}</p>
        <p className="text-meta text-muted-foreground">
          {resetText(secondsUntilReset(window, now))}
        </p>
      </div>
      <div className="flex items-baseline gap-2">
        <span
          className={cn(
            "text-ui font-medium tabular-nums",
            percentTextClass(tone),
          )}
        >
          {shown}
        </span>
        {toneText(tone) && (
          <span className={cn("text-meta", percentTextClass(tone))}>
            {toneText(tone)}
          </span>
        )}
      </div>
      <div
        role="progressbar"
        aria-label={name}
        aria-valuemin={0}
        aria-valuemax={100}
        aria-valuenow={Math.min(100, Math.round(fraction * 100))}
        aria-valuetext={shown}
        className="relative h-2 overflow-hidden rounded-full bg-muted"
      >
        <div
          className={cn("h-full", barFillClass(tone))}
          style={{ width: `${fraction * 100}%` }}
        />
        {ticks.map((position) => (
          <span
            key={position}
            aria-hidden
            className="absolute inset-y-0 w-px bg-background/80"
            style={{ left: `${position}%` }}
          />
        ))}
      </div>
      {pace && (
        <p className="text-meta text-muted-foreground">{paceText(pace)}</p>
      )}
    </section>
  );
}

function SignInNote({ name, harness }: { name: string; harness: HarnessId }) {
  const hint = signInHint(harness);
  return (
    <div className="grid gap-1.5 text-nested">
      <p>{copy.planLimits.signInDetail(name)}</p>
      {hint && <p className="text-muted-foreground">{hint}</p>}
    </div>
  );
}

function UnavailableNote({ reason }: { reason: string | null }) {
  return (
    <div className="grid gap-2 text-nested">
      <p className="font-medium">{copy.planLimits.unavailableTitle}</p>
      {reason && <p className="break-words text-muted-foreground">{reason}</p>}
      <div>
        <Button
          variant="outline"
          size="sm"
          onClick={() => {
            usePlanUsageStore
              .getState()
              .refresh(true)
              .catch(() => undefined);
          }}
        >
          {copy.planLimits.retry}
        </Button>
      </div>
    </div>
  );
}
