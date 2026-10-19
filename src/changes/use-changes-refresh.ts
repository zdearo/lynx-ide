import { useChangesStore } from "@/changes/changes-store";
import { useRefreshWhileVisible } from "@/lib/use-refresh-while-visible";

/** Agents write files all the time, and the window may stay focused while they do. */
const POLL_MS = 4000;

/** Keeps the changed files of a worktree fresh while a panel that shows them is on screen. */
export function useChangesRefresh(worktreePath: string | null) {
  useRefreshWhileVisible(
    worktreePath,
    (path) => void useChangesStore.getState().refresh(path),
    POLL_MS,
  );
}
