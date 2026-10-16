import { Loader2Glyph18 } from "@/components/icons/Loader2Glyph18";

import { cn } from "@/lib/utils";

export type DotState = "running" | "idle" | "creating";

/** Filled means something is running, hollow means nothing is: shape carries the state, not just color. */
export function StateDot({
  state,
  className,
}: {
  state: DotState;
  className?: string;
}) {
  if (state === "creating") {
    return (
      <Loader2Glyph18
        aria-hidden
        className={cn(
          "size-3 shrink-0 animate-spin text-muted-foreground",
          className,
        )}
      />
    );
  }
  return (
    <span
      aria-hidden
      className={cn(
        "size-2 shrink-0 rounded-full",
        state === "running"
          ? "bg-live"
          : "ring-1 ring-muted-foreground/60 ring-inset",
        className,
      )}
    />
  );
}
