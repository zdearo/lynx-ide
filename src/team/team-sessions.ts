import type { TerminalInfo } from "@/bindings";
import type { TeamMember } from "@/team/team-fixtures";

/** The session a member runs in, when it is an agent whose session is still running. Bots have no session. */
export function liveSessionOf(
  member: TeamMember,
  sessions: readonly TerminalInfo[],
): TerminalInfo | undefined {
  if (member.kind !== "agent" || member.sessionId === null) return undefined;
  return sessions.find(
    (session) => session.id === member.sessionId && session.running,
  );
}
