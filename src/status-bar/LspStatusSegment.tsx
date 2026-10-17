import { cn } from "@/lib/utils";
import { lspSummary } from "@/status-bar/lsp-summary";
import { useLspStatusStore } from "@/status-bar/lsp-status-store";

/** A filled dot when a server runs, a hollow one when none does; the words say it either way. */
export function LspStatusSegment() {
  const servers = useLspStatusStore((state) => state.servers);
  return (
    <span role="status" className="flex min-w-0 items-center gap-1.5 truncate">
      <span
        aria-hidden
        className={cn(
          "size-1.5 shrink-0 rounded-full",
          servers.length > 0 ? "bg-live" : "border border-muted-foreground",
        )}
      />
      <span className="truncate">{lspSummary(servers)}</span>
    </span>
  );
}
