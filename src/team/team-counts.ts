import type { TeamMember } from "@/team/team-fixtures";

/** How many members are working, and how many wait for the user. */
export function teamCounts(members: readonly TeamMember[]) {
  return {
    working: members.filter((member) => member.status === "working").length,
    needsYou: members.filter((member) => member.status === "needsYou").length,
  };
}
