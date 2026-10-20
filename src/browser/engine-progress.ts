import type { EngineProgress } from "@/bindings";
import { copy } from "@/ui-copy";

/** Megabytes as a person reads them, with no decimals. */
function megabytes(bytes: number): string {
  return `${Math.max(1, Math.round(bytes / 1_000_000))} MB`;
}

/** A line saying how far the install has got. */
export function describeEngineProgress(
  progress: EngineProgress | null,
): string {
  const text = copy.browser.engine;
  if (!progress) return text.preparing;
  switch (progress.phase) {
    case "downloading":
      return progress.total > 0
        ? text.downloading(
            megabytes(progress.received),
            megabytes(progress.total),
          )
        : text.preparing;
    case "verifying":
      return text.verifying;
    case "unpacking":
      return text.unpacking;
    case "done":
      return text.done;
  }
}

/** How much of the download is done, from 0 to 1, or null when it cannot be told. */
export function engineFraction(progress: EngineProgress | null): number | null {
  if (!progress || progress.phase === "unpacking") return null;
  if (progress.phase === "verifying" || progress.phase === "done") return 1;
  if (progress.total <= 0) return null;
  return Math.min(1, progress.received / progress.total);
}
