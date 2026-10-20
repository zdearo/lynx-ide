import type { Folder, Repo } from "@/bindings";

/**
 * Repos in the order their worktrees should load: the ones on screen first, then the ones
 * inside a collapsed folder. With many linked projects the visible ones fill in sooner.
 */
export function visibleFirst(
  repos: readonly Repo[],
  folders: readonly Folder[],
): Repo[] {
  const byId = new Map(folders.map((folder) => [folder.id, folder]));

  const hidden = (repo: Repo): boolean => {
    let current = repo.folderId;
    // A loop in the data would never end, so the walk is capped at the number of folders.
    for (let step = 0; current && step <= folders.length; step++) {
      const folder = byId.get(current);
      if (!folder) return false;
      if (folder.collapsed) return true;
      current = folder.parentId;
    }
    return false;
  };

  const shown = repos.filter((repo) => !hidden(repo));
  const covered = repos.filter(hidden);
  return [...shown, ...covered];
}
