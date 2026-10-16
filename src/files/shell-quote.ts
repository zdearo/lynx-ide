/**
 * A path as it should be typed at a prompt: left alone when it is plain, and
 * quoted when a shell would split or expand it.
 */
export function quotePathForShell(path: string, windows = false): string {
  if (windows) {
    return /^[\w./:\\-]+$/.test(path)
      ? path
      : `"${path.replaceAll('"', '\\"')}"`;
  }
  if (/^[\w@%+=:,./-]+$/.test(path)) return path;
  return `'${path.replaceAll("'", `'\\''`)}'`;
}
