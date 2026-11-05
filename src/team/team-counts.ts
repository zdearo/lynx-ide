import type { TerminalInfo } from "@/bindings";
import type { TeamMember } from "@/team/team-fixtures";
import { liveSessionOf } from "@/team/team-sessions";

/**
 * How many members are working, how many wait for the user, and how many are running and neither: a live agent that
 * is waiting. A done or closed member is not live, even if its session still runs.
 */
export function teamCounts(
  members: readonly TeamMember[],
  sessions: readonly TerminalInfo[],
) {
  const live = (member: TeamMember) =>
    liveSessionOf(member, sessions) !== undefined &&
    member.status !== "done" &&
    member.status !== "closed";
  return {
    working: members.filter((member) => member.status === "working").length,
    needsYou: members.filter((member) => member.status === "needsYou").length,
    // A live member's status is working, needsYou or idle, so idle is the waiting one.
    running: members.filter(
      (member) => live(member) && member.status === "idle",
    ).length,
  };
}
