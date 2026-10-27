import type { TeamMember } from "@/team/team-fixtures";

/** Card size in flow pixels. The cards are drawn at this size, so the layout places them without measuring. */
export const TEAM_CARD_WIDTH = 224;
export const TEAM_CARD_HEIGHT = 112;
const GAP_X = 32;
const GAP_Y = 64;

export interface TeamPosition {
  x: number;
  y: number;
}

/**
 * Where each member on screen goes in the org chart: a member centred over its own members, siblings
 * side by side, one row per level. A collapsed member's own members are left out, and so is a member
 * whose parent is not in the run.
 */
export function teamLayout(
  members: readonly TeamMember[],
  expanded: readonly string[],
): Map<string, TeamPosition> {
  const childrenOf = new Map<string, TeamMember[]>();
  for (const member of members) {
    if (member.parentId === null) continue;
    childrenOf.set(member.parentId, [
      ...(childrenOf.get(member.parentId) ?? []),
      member,
    ]);
  }
  const shownChildren = (id: string) =>
    expanded.includes(id) ? (childrenOf.get(id) ?? []) : [];

  // Only the members reached from a root are placed, so a cycle outside the run is never walked.
  function rowWidth(row: readonly TeamMember[]): number {
    return (
      row.reduce((sum, member) => sum + subtreeWidth(member.id), 0) +
      GAP_X * Math.max(0, row.length - 1)
    );
  }
  function subtreeWidth(id: string): number {
    return Math.max(TEAM_CARD_WIDTH, rowWidth(shownChildren(id)));
  }

  const positions = new Map<string, TeamPosition>();
  function placeRow(
    row: readonly TeamMember[],
    left: number,
    span: number,
    depth: number,
  ) {
    let cursor = left + (span - rowWidth(row)) / 2;
    for (const member of row) {
      const width = subtreeWidth(member.id);
      positions.set(member.id, {
        x: cursor + (width - TEAM_CARD_WIDTH) / 2,
        y: depth * (TEAM_CARD_HEIGHT + GAP_Y),
      });
      placeRow(shownChildren(member.id), cursor, width, depth + 1);
      cursor += width + GAP_X;
    }
  }

  const roots = members.filter((member) => member.parentId === null);
  placeRow(roots, 0, rowWidth(roots), 0);
  return positions;
}
