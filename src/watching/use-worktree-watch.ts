import { useEffect } from "react";

import { commands } from "@/bindings";
import { unwrap } from "@/lib/ipc-result";
import { useWatchStore } from "@/watching/watch-store";

/**
 * Asks the backend to watch the worktree on screen, so its files are read again as they change.
 * Where watching is not possible the store says so and the panels keep their timer.
 */
export function useWorktreeWatch(worktreePath: string | null) {
  useEffect(() => {
    let current = true;
    useWatchStore.getState().setWatched(null);
    unwrap(commands.watchWorktree(worktreePath))
      .then((watching) => {
        if (current && watching) {
          useWatchStore.getState().setWatched(worktreePath);
        }
      })
      // Not watching is a slower screen, not a failure worth showing.
      .catch(() => undefined);
    return () => {
      current = false;
    };
  }, [worktreePath]);
}
