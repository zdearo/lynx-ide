import type { CardSize } from "@/team/team-canvas-model";
import type { TeamPosition } from "@/team/team-layout";

/** The clear space between two nodes that push each other apart, in canvas units. */
export const PUSH_GAP = 24;

export interface PushItem {
  id: string;
  corner: TeamPosition;
  size: CardSize;
}

/**
 * Lays the nodes out so none overlaps another, with PUSH_GAP between them. The nodes the layout puts at the same height
 * are one row: each row is packed from left to right, a node that overlaps the one before it moving to its right. A row
 * that changed is then centred on where it was, so its neighbours make room on both sides. Rows stack from the top,
 * and a row that grew moves the rows under it down. It is pure: the caller computes it from the layout each time, so
 * closing a terminal brings the cards back to their places.
 */
export function pushApart(
  items: readonly PushItem[],
  gap = PUSH_GAP,
): Map<string, TeamPosition> {
  const rows = new Map<number, PushItem[]>();
  for (const item of items) {
    rows.set(item.corner.y, [...(rows.get(item.corner.y) ?? []), item]);
  }
  const placed = new Map<string, TeamPosition>();
  // The lowest bottom of the rows above, with the gap: no row starts above it.
  let floor = -Infinity;
  for (const y of [...rows.keys()].sort((a, b) => a - b)) {
    const row = [...rows.get(y)!].sort((a, b) => a.corner.x - b.corner.x);
    const top = Math.max(y, floor);
    const xs = new Map<string, number>();
    let right = -Infinity;
    let moved = false;
    for (const item of row) {
      const x = Math.max(item.corner.x, right + gap);
      xs.set(item.id, x);
      if (x !== item.corner.x) moved = true;
      right = x + item.size.width;
    }
    const originalRight = Math.max(
      ...row.map((item) => item.corner.x + item.size.width),
    );
    const shift = moved ? (originalRight - right) / 2 : 0;
    let bottom = -Infinity;
    for (const item of row) {
      placed.set(item.id, { x: (xs.get(item.id) ?? 0) + shift, y: top });
      bottom = Math.max(bottom, top + item.size.height);
    }
    floor = Math.max(floor, bottom + gap);
  }
  return placed;
}
