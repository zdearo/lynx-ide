import { useUsageConfig } from "@/config/config-store";
import { useRefreshWhileVisible } from "@/lib/use-refresh-while-visible";
import { copy } from "@/ui-copy";
import { PlanUsageSegment } from "@/usage/PlanUsageSegment";
import { usePlanUsageStore } from "@/usage/plan-usage-store";

/** Usage moves slowly. Focusing the window also reads it, and the backend keeps reads a minute apart. */
const POLL_MS = 5 * 60 * 1000;

/** One button per plan, on the left of the status bar. Nothing shows while the limits are off or unreadable. */
export function PlanUsageSegments() {
  const config = useUsageConfig();
  const entries = usePlanUsageStore((state) => state.entries);
  // The target names what is read: turning a plan on or off reads at once, and turning the limits off stops the poll.
  const target = config.visible ? config.providers.join(",") : null;
  useRefreshWhileVisible(
    target,
    () => {
      // A failed read keeps the numbers already on screen.
      usePlanUsageStore
        .getState()
        .refresh(false)
        .catch(() => undefined);
    },
    POLL_MS,
  );

  if (!config.visible || !entries || entries.length === 0) return null;
  return (
    <div
      role="group"
      aria-label={copy.planLimits.title}
      className="flex min-w-0 items-center gap-3"
    >
      {entries.map((usage) => (
        <PlanUsageSegment key={usage.harness} usage={usage} config={config} />
      ))}
    </div>
  );
}
