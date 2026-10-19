/**
 * A file of an old commit opens in its own tab, next to the working-tree file of the same path.
 * The tab's id carries the hash so the two never collide; the real path travels with it.
 */
export function commitFileId(hash: string, path: string): string {
  return `commit:${hash}:${path}`;
}
