import type { TeamMember } from "@/team/team-fixtures";
import {
  TEAM_CARD_HEIGHT,
  TEAM_CARD_WIDTH,
  type TeamPosition,
} from "@/team/team-layout";
import type { TeamPair } from "@/team/team-pairs";

/** The size of a card on the canvas, in canvas units. */
export interface CardSize {
  width: number;
  height: number;
}

/** A card that was not resized. The layout places every card at this size. */
export const CARD_DEFAULT_SIZE: CardSize = {
  width: TEAM_CARD_WIDTH,
  height: TEAM_CARD_HEIGHT,
};
/** The smallest a card may be resized to: a title, a status line and a task still fit. */
export const CARD_MIN_SIZE: CardSize = { width: 180, height: 96 };
/** The largest a card may be resized to, so a terminal-sized card cannot cover the team. */
export const CARD_MAX_SIZE: CardSize = { width: 560, height: 420 };

/** A size held between the smallest and the largest a card may be. */
export function clampSize(size: CardSize): CardSize {
  return {
    width: Math.min(
      CARD_MAX_SIZE.width,
      Math.max(CARD_MIN_SIZE.width, Math.round(size.width)),
    ),
    height: Math.min(
      CARD_MAX_SIZE.height,
      Math.max(CARD_MIN_SIZE.height, Math.round(size.height)),
    ),
  };
}

/** The size a member's card is drawn at: the one the user gave it, or the default. */
export function sizeOf(
  sizes: Readonly<Record<string, CardSize>>,
  id: string,
): CardSize {
  return sizes[id] ?? CARD_DEFAULT_SIZE;
}

/** Cytoscape places a node by its centre; the team places a card by its top-left corner. */
export function centreOf(
  corner: TeamPosition,
  size: CardSize = CARD_DEFAULT_SIZE,
): TeamPosition {
  return {
    x: corner.x + size.width / 2,
    y: corner.y + size.height / 2,
  };
}

/** The top-left corner of a card of this size whose centre is `centre`. */
export function cornerOf(
  centre: TeamPosition,
  size: CardSize = CARD_DEFAULT_SIZE,
): TeamPosition {
  return {
    x: centre.x - size.width / 2,
    y: centre.y - size.height / 2,
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

/** The four handles of a card: the top and bottom ones make an orchestration, the side ones a pair. */
export type HandleName = "top" | "bottom" | "left" | "right";

/** The handle a line lands on: the opposite side of the one it leaves from. */
export const OPPOSITE_HANDLE: Record<HandleName, HandleName> = {
  top: "bottom",
  bottom: "top",
  left: "right",
  right: "left",
};

/** Where a handle sits on a card of this size whose top-left corner is `corner`, in canvas units. */
export function handleAnchor(
  corner: TeamPosition,
  handle: HandleName,
  size: CardSize = CARD_DEFAULT_SIZE,
): TeamPosition {
  switch (handle) {
    case "top":
      return { x: corner.x + size.width / 2, y: corner.y };
    case "bottom":
      return { x: corner.x + size.width / 2, y: corner.y + size.height };
    case "left":
      return { x: corner.x, y: corner.y + size.height / 2 };
    case "right":
      return { x: corner.x + size.width, y: corner.y + size.height / 2 };
  }
}
