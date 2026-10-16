import type { TerminalInfo } from "@/bindings";

function baseTitle(session: TerminalInfo): string {
  // The harness names a session that became an agent by hand, which has no command of its own.
  if (session.harness) return session.harness;
  if (!session.command) return session.shell;
  return session.command.trim().split(/\s+/)[0] ?? session.shell;
}

/** Tab titles for one worktree. Repeats are numbered in creation order: `claude`, `claude 2`. */
export function tabTitles(
  sessions: readonly TerminalInfo[],
): Map<string, string> {
  const seen = new Map<string, number>();
  const titles = new Map<string, string>();
  for (const session of sessions) {
    const base = baseTitle(session);
    const count = (seen.get(base) ?? 0) + 1;
    seen.set(base, count);
    titles.set(session.id, count === 1 ? base : `${base} ${count}`);
  }
  return titles;
}
