import { copy } from "@/ui-copy";

/** How long ago a thread was last active, short enough for the end of a row. Times are Unix seconds. */
export function threadAge(updatedAt: number, now: number): string {
  const text = copy.bots.threads.age;
  const seconds = Math.max(0, now - updatedAt);
  if (seconds < 60) return text.now;
  const minutes = Math.floor(seconds / 60);
  if (minutes < 60) return text.minutes(minutes);
  const hours = Math.floor(minutes / 60);
  if (hours < 24) return text.hours(hours);
  return text.days(Math.floor(hours / 24));
}
