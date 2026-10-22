import type { DaemonStatus } from "@/bindings";
import { copy } from "@/ui-copy";

/** What the status bar says about the terminal process; null while the setting is off. */
export function daemonSummary(status: DaemonStatus): string | null {
  if (!status.enabled) return null;
  const text = copy.statusBar.daemon;
  switch (status.state) {
    case "idle":
      return text.idle;
    case "connected":
      return text.connected(status.sessions);
    case "degraded":
      return text.degraded;
  }
}
