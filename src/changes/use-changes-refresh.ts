import { useEffect } from "react";

import { useChangesStore } from "@/changes/changes-store";

/** Agents write files all the time, and the window may stay focused while they do. */
const POLL_MS = 4000;

/** Keeps the changed files of a worktree fresh while a panel that shows them is on screen. */
export function useChangesRefresh(worktreePath: string | null) {
  useEffect(() => {
    if (!worktreePath) return;
    const refresh = () => void useChangesStore.getState().refresh(worktreePath);
    refresh();
    const timer = window.setInterval(() => {
      if (document.visibilityState === "visible") refresh();
    }, POLL_MS);
    window.addEventListener("focus", refresh);
    return () => {
      window.clearInterval(timer);
      window.removeEventListener("focus", refresh);
    };
  }, [worktreePath]);
}
