import type { TeamMember } from "@/team/team-fixtures";

export interface TeamRow {
  member: TeamMember;
  depth: number;
  hasChildren: boolean;
  expanded: boolean;
}

/**
 * The members on screen, top to bottom. A member's own members follow it only while it is
 * expanded. A member whose parent is gone is not shown.
 */
export function teamRows(
  members: readonly TeamMember[],
  expanded: readonly string[],
): TeamRow[] {
  const childrenOf = new Map<string | null, TeamMember[]>();
  for (const member of members) {
    const siblings = childrenOf.get(member.parentId) ?? [];
    siblings.push(member);
    childrenOf.set(member.parentId, siblings);
  }

  const rows: TeamRow[] = [];
  const walk = (parentId: string | null, depth: number) => {
    for (const member of childrenOf.get(parentId) ?? []) {
      const hasChildren = childrenOf.has(member.id);
      const open = hasChildren && expanded.includes(member.id);
      rows.push({ member, depth, hasChildren, expanded: open });
      if (open) walk(member.id, depth + 1);
    }
  };
  walk(null, 0);
  return rows;
}

/** How many members are working, and how many wait for the user. */
export function teamCounts(members: readonly TeamMember[]) {
  return {
    working: members.filter((member) => member.status === "working").length,
    needsYou: members.filter((member) => member.status === "needsYou").length,
  };
}
