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
/** The largest a card may be resized to. */
export const CARD_MAX_SIZE: CardSize = { width: 560, height: 420 };
/** A member's terminal on the map when it is open and was not resized: a terminal that reads well. */
export const TERMINAL_DEFAULT_SIZE: CardSize = { width: 480, height: 300 };
export const TERMINAL_MIN_SIZE: CardSize = { width: 320, height: 180 };
export const TERMINAL_MAX_SIZE: CardSize = { width: 960, height: 720 };

function clampTo(size: CardSize, min: CardSize, max: CardSize): CardSize {
  return {
    width: Math.min(max.width, Math.max(min.width, Math.round(size.width))),
    height: Math.min(max.height, Math.max(min.height, Math.round(size.height))),
  };
}

/** A card's size held between the smallest and the largest a card may be. */
export function clampSize(size: CardSize): CardSize {
  return clampTo(size, CARD_MIN_SIZE, CARD_MAX_SIZE);
}

/** A terminal's size held between the smallest and the largest a terminal may be. */
export function clampTerminalSize(size: CardSize): CardSize {
  return clampTo(size, TERMINAL_MIN_SIZE, TERMINAL_MAX_SIZE);
}

/** The size a member's node is drawn at: the one the user gave it, or the default for its kind. */
export function sizeOf(
  sizes: Readonly<Record<string, CardSize>>,
  id: string,
  fallback: CardSize = CARD_DEFAULT_SIZE,
): CardSize {
  return sizes[id] ?? fallback;
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

/** A card's box on the map, from its two corners. */
export interface Box {
  x1: number;
  y1: number;
  x2: number;
  y2: number;
}

/** The pan that puts a map point at the middle of the area, at zoom 1. */
export function centredPan(
  point: { x: number; y: number },
  area: CardSize,
): { x: number; y: number } {
  return { x: area.width / 2 - point.x, y: area.height / 2 - point.y };
}

/** A card's box on the map, from its centre and size. */
export function boxOf(centre: { x: number; y: number }, size: CardSize): Box {
  return {
    x1: centre.x - size.width / 2,
    y1: centre.y - size.height / 2,
    x2: centre.x + size.width / 2,
    y2: centre.y + size.height / 2,
  };
}

/** The zoom and pan that fit the boxes in the area with the padding around them; the zoom stays in range. */
export function fitView(
  boxes: readonly Box[],
  area: CardSize,
  padding: number,
  zoomRange: { min: number; max: number },
): { zoom: number; pan: { x: number; y: number } } {
  if (boxes.length === 0) return { zoom: 1, pan: { x: 0, y: 0 } };
  const x1 = Math.min(...boxes.map((box) => box.x1));
  const y1 = Math.min(...boxes.map((box) => box.y1));
  const x2 = Math.max(...boxes.map((box) => box.x2));
  const y2 = Math.max(...boxes.map((box) => box.y2));
  const fitting = Math.min(
    (area.width - 2 * padding) / Math.max(x2 - x1, 1),
    (area.height - 2 * padding) / Math.max(y2 - y1, 1),
  );
  const zoom = Math.min(Math.max(fitting, zoomRange.min), zoomRange.max);
  return {
    zoom,
    pan: {
      x: area.width / 2 - (zoom * (x1 + x2)) / 2,
      y: area.height / 2 - (zoom * (y1 + y2)) / 2,
    },
  };
}

/** Whether a box of the map is on the screen at all, at this zoom and pan. */
export function isInView(
  box: Box,
  zoom: number,
  pan: { x: number; y: number },
  area: CardSize,
): boolean {
  return (
    box.x2 * zoom + pan.x > 0 &&
    box.x1 * zoom + pan.x < area.width &&
    box.y2 * zoom + pan.y > 0 &&
    box.y1 * zoom + pan.y < area.height
  );
}
