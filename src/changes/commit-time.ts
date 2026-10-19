import { copy } from "@/ui-copy";

/** "5 min ago" for a commit time in seconds since 1970, or the date once it is a month old. */
export function relativeTime(timestamp: number, nowMs: number): string {
  const seconds = Math.max(0, Math.round(nowMs / 1000 - timestamp));
  if (seconds < 60) return copy.commits.ago.now;
  const minutes = Math.floor(seconds / 60);
  if (minutes < 60) return copy.commits.ago.minutes(minutes);
  const hours = Math.floor(minutes / 60);
  if (hours < 24) return copy.commits.ago.hours(hours);
  const days = Math.floor(hours / 24);
  if (days < 30) return copy.commits.ago.days(days);
  return new Date(timestamp * 1000).toISOString().slice(0, 10);
}
