import { copy } from "@/ui-copy";
import { LspStatusSegment } from "@/status-bar/LspStatusSegment";

/** A low strip under the whole app. Agent limits will sit on the left; the language servers on the right. */
export function StatusBar() {
  return (
    <footer
      aria-label={copy.statusBar.label}
      className="flex h-6 shrink-0 items-center justify-between gap-3 border-t border-sidebar-border bg-sidebar px-3 text-meta text-muted-foreground"
    >
      <div className="min-w-0" />
      <LspStatusSegment />
    </footer>
  );
}
