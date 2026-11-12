import type { TeamMember } from "@/team/team-fixtures";
import { partnersOf, type TeamPair } from "@/team/team-pairs";

/**
 * The members a member talks with: its pairs, the orchestrator above it and the members under it. A bot can be one
 * of them.
 */
export function talksToOf(
  member: TeamMember,
  members: readonly TeamMember[],
  pairs: readonly TeamPair[],
): TeamMember[] {
  const ids = new Set<string | null>([
    ...partnersOf(pairs, member.id),
    member.parentId,
    ...members
      .filter((other) => other.parentId === member.id)
      .map((other) => other.id),
  ]);
  return members.filter((other) => other.id !== member.id && ids.has(other.id));
}
