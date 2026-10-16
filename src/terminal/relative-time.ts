/** "now", "5m", "3h", "2d": how long ago something started, as short as a list row needs. */
export function formatAge(startedAtSeconds: number, nowMs: number): string {
  const seconds = Math.max(0, Math.floor(nowMs / 1000 - startedAtSeconds));
  if (seconds < 60) return "now";
  if (seconds < 3600) return `${Math.floor(seconds / 60)}m`;
  if (seconds < 86400) return `${Math.floor(seconds / 3600)}h`;
  return `${Math.floor(seconds / 86400)}d`;
}
