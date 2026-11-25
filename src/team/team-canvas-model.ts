import type { TeamMember } from "@/team/team-fixtures";
import {
  TEAM_CARD_HEIGHT,
  TEAM_CARD_WIDTH,
  type TeamPosition,
} from "@/team/team-layout";
import type { TeamPair } from "@/team/team-pairs";

/** Cytoscape places a node by its centre; the team places a card by its top-left corner. */
export function centreOf(corner: TeamPosition): TeamPosition {
  return {
    x: corner.x + TEAM_CARD_WIDTH / 2,
    y: corner.y + TEAM_CARD_HEIGHT / 2,
  };
}

/** The top-left corner of a card whose centre is `centre`. */
export function cornerOf(centre: TeamPosition): TeamPosition {
  return {
    x: centre.x - TEAM_CARD_WIDTH / 2,
    y: centre.y - TEAM_CARD_HEIGHT / 2,
  };
}

/** The transform that puts a card at its corner on screen: the canvas's zoom and pan, applied to the corner. */
export function cardTransform(
  corner: TeamPosition,
  zoom: number,
  pan: TeamPosition,
): string {
  return `translate(${corner.x * zoom + pan.x}px, ${corner.y * zoom + pan.y}px) scale(${zoom})`;
}

/** The selected member, the members above it, and its partners: what stays in focus. Null with no selection. */
export function focusOf(
  members: readonly TeamMember[],
  pairs: readonly TeamPair[],
  selectedId: string | null,
): Set<string> | null {
  const selected = members.find((member) => member.id === selectedId);
  if (!selected) return null;
  const focus = new Set([selected.id]);
  let parentId = selected.parentId;
  while (parentId && !focus.has(parentId)) {
    focus.add(parentId);
    parentId =
      members.find((member) => member.id === parentId)?.parentId ?? null;
  }
  for (const pair of pairs) {
    if (pair.from === selected.id) focus.add(pair.to);
    if (pair.to === selected.id) focus.add(pair.from);
  }
  return focus;
}
