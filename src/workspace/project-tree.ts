import type { Folder, Repo } from "@/bindings";

export type ProjectNode =
  | { kind: "folder"; folder: Folder; children: ProjectNode[] }
  | { kind: "repo"; repo: Repo };

const byName = (a: Folder, b: Folder) =>
  a.name.localeCompare(b.name, undefined, {
    sensitivity: "base",
    numeric: true,
  });

/**
 * The sidebar's tree. At each level folders come first, sorted by name, and
 * repos follow in the order they were added. Nothing is lost to bad data: a
 * folder whose parent is missing, or that sits in a loop, is shown at the top.
 */
export function buildProjectTree(
  folders: readonly Folder[],
  repos: readonly Repo[],
): ProjectNode[] {
  const known = new Set(folders.map((folder) => folder.id));
  const folderParent = new Map<string, string | null>(
    folders.map((folder) => [
      folder.id,
      folder.parentId && known.has(folder.parentId) ? folder.parentId : null,
    ]),
  );

  // Folders that cannot be reached from the top are part of a loop; lift them up.
  const reachable = new Set<string>();
  const visit = (parent: string | null) => {
    for (const folder of folders) {
      if (folderParent.get(folder.id) === parent && !reachable.has(folder.id)) {
        reachable.add(folder.id);
        visit(folder.id);
      }
    }
  };
  visit(null);
  for (const folder of folders) {
    if (!reachable.has(folder.id)) folderParent.set(folder.id, null);
  }

  const repoParent = (repo: Repo) =>
    repo.folderId && reachable.has(repo.folderId) ? repo.folderId : null;

  const build = (parent: string | null): ProjectNode[] => [
    ...folders
      .filter((folder) => folderParent.get(folder.id) === parent)
      .sort(byName)
      .map((folder) => ({
        kind: "folder" as const,
        folder,
        children: build(folder.id),
      })),
    ...repos
      .filter((repo) => repoParent(repo) === parent)
      .map((repo) => ({ kind: "repo" as const, repo })),
  ];

  return build(null);
}

/** Every folder in tree order, with how deep it sits (0 at the top). */
export function flattenFolders(
  folders: readonly Folder[],
): Array<{ folder: Folder; depth: number }> {
  const out: Array<{ folder: Folder; depth: number }> = [];
  const walk = (nodes: ProjectNode[], depth: number) => {
    for (const node of nodes) {
      if (node.kind !== "folder") continue;
      out.push({ folder: node.folder, depth });
      walk(node.children, depth + 1);
    }
  };
  walk(buildProjectTree(folders, []), 0);
  return out;
}

/** A folder and everything inside it, at any depth. Used to stop a folder moving into itself. */
export function folderAndDescendantIds(
  folders: readonly Folder[],
  folderId: string,
): Set<string> {
  const ids = new Set<string>([folderId]);
  let grew = true;
  while (grew) {
    grew = false;
    for (const folder of folders) {
      if (folder.parentId && ids.has(folder.parentId) && !ids.has(folder.id)) {
        ids.add(folder.id);
        grew = true;
      }
    }
  }
  return ids;
}
