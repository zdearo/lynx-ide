import { useRefreshWhileVisible } from "@/lib/use-refresh-while-visible";
import { isLinkedRoot } from "@/workspace/projects/linked-folders";
import { useWorkspaceStore } from "@/workspace/workspace-store";

/** Linked directories change when the user clones a repo, so the scan is repeated while the window is on screen. */
const SYNC_MS = 60_000;

/**
 * Keeps linked folders in step with their directories: once the workspace has loaded, again every
 * minute, and when the window gets focus. A scan reads directory names only, so it is cheap.
 */
export function useLinkedFolderSync() {
  const active = useWorkspaceStore(
    (state) => state.loaded && state.folders.some(isLinkedRoot),
  );
  useRefreshWhileVisible(
    active ? "linked" : null,
    () => {
      // A failed scan is retried on the next tick; a toast every minute would only nag.
      void useWorkspaceStore
        .getState()
        .syncLinkedFolders()
        .catch(() => undefined);
    },
    SYNC_MS,
  );
}
