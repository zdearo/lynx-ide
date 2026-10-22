import { copy } from "@/ui-copy";
import { DaemonStatusSegment } from "@/status-bar/DaemonStatusSegment";
import { LspStatusSegment } from "@/status-bar/LspStatusSegment";
import { PlanUsageSegments } from "@/usage/PlanUsageSegments";

/** A low strip under the whole app. Plan limits sit on the left; the language servers on the right. */
export function StatusBar() {
  return (
    <footer
      aria-label={copy.statusBar.label}
      className="flex h-6 shrink-0 items-center justify-between gap-3 border-t border-sidebar-border bg-sidebar px-3 text-meta text-muted-foreground"
    >
      <PlanUsageSegments />
      <div className="flex min-w-0 items-center gap-3">
        <DaemonStatusSegment />
        <LspStatusSegment />
      </div>
    </footer>
  );
}
