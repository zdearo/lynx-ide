import type { Folder } from "@/bindings";

/** A folder the user linked to a directory on disk. */
export const isLinkedRoot = (folder: Folder): boolean =>
  folder.linkedRootId === folder.id;

/** A folder made by a linked folder's scan; it cannot be deleted on its own. */
export const isMirrored = (folder: Folder): boolean =>
  folder.linkedRootId !== null && folder.linkedRootId !== folder.id;

/** The last part of a path, for naming a folder after the directory it follows. */
export function directoryNameOf(path: string): string {
  const parts = path.split(/[\\/]+/).filter((part) => part.length > 0);
  return parts.at(-1) ?? path;
}
