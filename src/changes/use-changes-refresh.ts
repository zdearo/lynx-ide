import { refreshWorktreeGit } from "@/changes/refresh-worktree-git";
import { useRefreshWhileVisible } from "@/lib/use-refresh-while-visible";
import { useWatchStore } from "@/watching/watch-store";

/** Agents write files all the time, and the window may stay focused while they do. */
const POLL_MS = 4000;
/** While files are watched the timer is only a safety net: for a remote that moved, or an event the system dropped. */
const WATCHED_POLL_MS = 30_000;

/** Keeps the changed files of a worktree fresh while a panel that shows them is on screen. */
export function useChangesRefresh(worktreePath: string | null) {
  const watched = useWatchStore((state) => state.watched === worktreePath);
  useRefreshWhileVisible(
    worktreePath,
    (path) => void refreshWorktreeGit(path),
    watched ? WATCHED_POLL_MS : POLL_MS,
  );
}
