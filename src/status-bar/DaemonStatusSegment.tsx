import { useEffect, useState } from "react";

import { commands, type DaemonStatus } from "@/bindings";
import { cn } from "@/lib/utils";
import { copy } from "@/ui-copy";
import { daemonSummary } from "@/status-bar/daemon-status-summary";

const POLL_MS = 5000;

/** The terminal process, when the setting is on: a filled dot while it serves, a hollow one when not. */
export function DaemonStatusSegment() {
  const [status, setStatus] = useState<DaemonStatus | null>(null);
  const [memory, setMemory] = useState<number | null>(null);

  // Memory is measured only when someone asks for it, by hovering: a poll would scan the processes every few seconds.
  const measure = () => {
    void commands.ptyDaemonMemory().then((result) => {
      if (result.status === "ok") setMemory(result.data);
    });
  };

  useEffect(() => {
    let active = true;
    const read = () => {
      void commands.ptyDaemonStatus().then((result) => {
        if (active && result.status === "ok") setStatus(result.data);
      });
    };
    read();
    const timer = setInterval(read, POLL_MS);
    return () => {
      active = false;
      clearInterval(timer);
    };
  }, []);

  const summary = status ? daemonSummary(status) : null;
  if (summary === null || status === null) return null;
  return (
    <span
      role="status"
      className="flex min-w-0 items-center gap-1.5 truncate"
      onMouseEnter={measure}
      title={memory === null ? undefined : copy.statusBar.daemon.memory(memory)}
    >
      <span
        aria-hidden
        className={cn(
          "size-1.5 shrink-0 rounded-full",
          status.state === "connected"
            ? "bg-live"
            : "border border-muted-foreground",
        )}
      />
      <span className="truncate">{summary}</span>
    </span>
  );
}
