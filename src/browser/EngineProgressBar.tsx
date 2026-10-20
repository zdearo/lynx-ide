import {
  describeEngineProgress,
  engineFraction,
} from "@/browser/engine-progress";
import { useBrowserStore } from "@/browser/browser-store";

/** The install's progress: a bar, and a line saying what is happening. */
export function EngineProgressBar() {
  const progress = useBrowserStore((s) => s.engineProgress);
  const fraction = engineFraction(progress);
  const label = describeEngineProgress(progress);

  return (
    <div className="grid gap-1.5">
      <div
        role="progressbar"
        aria-label={label}
        aria-valuemin={0}
        aria-valuemax={100}
        aria-valuenow={
          fraction === null ? undefined : Math.round(fraction * 100)
        }
        className="h-1.5 overflow-hidden rounded-full bg-muted"
      >
        <div
          className={
            fraction === null
              ? "h-full w-1/3 animate-pulse rounded-full bg-agent"
              : "h-full rounded-full bg-agent transition-[width]"
          }
          style={
            fraction === null ? undefined : { width: `${fraction * 100}%` }
          }
        />
      </div>
      <p className="text-meta text-muted-foreground">{label}</p>
    </div>
  );
}
