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
 * Moves the nodes apart until none overlaps another, with PUSH_GAP between them. The first item stays where it is,
 * and each later one moves out of every item before it that it overlaps: sideways when it sits in that item's row,
 * up or down otherwise.
 * It is pure: the caller pushes from the layout each time, so a node that closes lets its neighbours go back.
 */
export function pushApart(
  items: readonly PushItem[],
): Map<string, TeamPosition> {
  const boxes = items.map(({ id, corner, size }) => ({
    id,
    x: corner.x,
    y: corner.y,
    w: size.width,
    h: size.height,
  }));
  for (let round = 0; round < 50; round++) {
    let moved = false;
    for (let i = 1; i < boxes.length; i++) {
      const a = boxes[i];
      for (let j = 0; j < i; j++) {
        const b = boxes[j];
        const overlaps =
          a.x < b.x + b.w + PUSH_GAP &&
          b.x < a.x + a.w + PUSH_GAP &&
          a.y < b.y + b.h + PUSH_GAP &&
          b.y < a.y + a.h + PUSH_GAP;
        if (!overlaps) continue;
        const right = b.x + b.w + PUSH_GAP - a.x;
        const left = b.x - PUSH_GAP - a.w - a.x;
        const down = b.y + b.h + PUSH_GAP - a.y;
        const up = b.y - PUSH_GAP - a.h - a.y;
        // A node that starts in the upper half of the other one goes sideways, as a neighbour in its row does; any
        // other goes up or down. Either way it takes the shorter of its two moves.
        const shorter = (first: number, second: number) =>
          Math.abs(first) <= Math.abs(second) ? first : second;
        if (a.y < b.y + b.h / 2) a.x += shorter(right, left);
        else a.y += shorter(down, up);
        moved = true;
      }
    }
    if (!moved) break;
  }
  return new Map(boxes.map((box) => [box.id, { x: box.x, y: box.y }]));
}
