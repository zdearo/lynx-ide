import { useBranchSyncStore } from "@/changes/branch-sync-store";
import { useChangesStore } from "@/changes/changes-store";
import { useCommitsStore } from "@/changes/commits/commits-store";

interface Reading {
  /** Someone asked again while this one was running, so what it read may already be old. */
  again: boolean;
  done: Promise<void>;
}

const reading = new Map<string, Reading>();

function readOnce(worktreePath: string) {
  return Promise.all([
    useChangesStore.getState().refresh(worktreePath),
    useBranchSyncStore.getState().refresh(worktreePath),
    useCommitsStore.getState().refresh(worktreePath),
  ]);
}

/**
 * Everything the Changes tab shows about a worktree's Git state, read again together. Timers,
 * window focus, the file watcher and saves all ask for this, often at once; each worktree reads
 * once at a time, and any asks made meanwhile share one more read that starts afterwards.
 */
export function refreshWorktreeGit(worktreePath: string): Promise<unknown> {
  const current = reading.get(worktreePath);
  if (current) {
    current.again = true;
    return current.done;
  }
  const job: Reading = { again: false, done: Promise.resolve() };
  reading.set(worktreePath, job);
  job.done = (async () => {
    try {
      do {
        job.again = false;
        await readOnce(worktreePath);
      } while (job.again);
    } finally {
      reading.delete(worktreePath);
    }
  })();
  return job.done;
}
