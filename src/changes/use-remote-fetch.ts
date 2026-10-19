import { commands } from "@/bindings";
import { refreshWorktreeGit } from "@/changes/refresh-worktree-git";
import { unwrap } from "@/lib/ipc-result";
import { useRefreshWhileVisible } from "@/lib/use-refresh-while-visible";

/** A fetch reaches the network, so once a minute is plenty for "behind" to stay honest. */
const FETCH_MS = 60_000;

/** Asks the remote for news while the Changes tab is on screen, then reads the branch state again. */
export function useRemoteFetch(worktreePath: string | null) {
  useRefreshWhileVisible(
    worktreePath,
    (path) =>
      void (async () => {
        try {
          await unwrap(commands.fetchRemote(path));
        } catch {
          // Offline or no remote: the branch state is still worth reading.
        }
        await refreshWorktreeGit(path);
      })(),
    FETCH_MS,
  );
}
