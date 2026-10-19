import type { TreeState } from "@/files/file-tree-rows";

interface ChangedFolders {
  directories: readonly string[];
  everything: boolean;
}

/** The folders of the explorer to list again: only the ones that have been listed, so none is read for nothing. */
export function directoriesToReload(
  tree: TreeState | undefined,
  change: ChangedFolders,
): string[] {
  if (!tree) return [];
  const listed = Object.keys(tree.directories);
  return change.everything
    ? listed
    : change.directories.filter((directory) => directory in tree.directories);
}
