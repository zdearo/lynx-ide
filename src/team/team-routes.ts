import {
  TEAM_CARD_HEIGHT as H,
  TEAM_CARD_WIDTH as W,
  TEAM_ROW_GAP,
  type TeamPosition,
} from "@/team/team-layout";

type Point = TeamPosition;

const HALF_GAP = TEAM_ROW_GAP / 2;
/** How far a line keeps from a card it passes. */
const CLEARANCE = 4;
/** How far above a row a line runs to pass over the cards between a pair. */
const LIFT = 14;

/** Whether the straight, axis-aligned segment from a to b crosses the card whose top-left is `card`. */
function crosses(a: Point, b: Point, card: Point): boolean {
  return (
    Math.max(a.x, b.x) > card.x - CLEARANCE &&
    Math.min(a.x, b.x) < card.x + W + CLEARANCE &&
    Math.max(a.y, b.y) > card.y - CLEARANCE &&
    Math.min(a.y, b.y) < card.y + H + CLEARANCE
  );
}

export function routeIsClear(
  points: readonly Point[],
  cards: readonly Point[],
): boolean {
  return points.every(
    (point, i) =>
      i === 0 || cards.every((card) => !crosses(points[i - 1], point, card)),
  );
}

/** The x positions where a vertical line fits between the cards, and just outside them. */
function channelsAmong(cards: readonly Point[]): number[] {
  const spans = cards
    .map((card) => [card.x, card.x + W] as const)
    .sort((a, b) => a[0] - b[0]);
  const merged: [number, number][] = [];
  for (const [from, to] of spans) {
    const last = merged[merged.length - 1];
    if (last && from <= last[1]) last[1] = Math.max(last[1], to);
    else merged.push([from, to]);
  }
  if (merged.length === 0) return [];
  const xs = [merged[0][0] - 16, merged[merged.length - 1][1] + 16];
  merged.slice(1).forEach(([from], i) => {
    // A gap needs room for the line and its clearance on both sides.
    if (from - merged[i][1] > 2 * CLEARANCE + 2) {
      xs.push((from + merged[i][1]) / 2);
    }
  });
  return xs;
}

function tidy(points: Point[]): Point[] {
  return points.filter(
    (p, i) => i === 0 || p.x !== points[i - 1].x || p.y !== points[i - 1].y,
  );
}

/** The cheapest of the routes that clear every card, or the first when none does. */
function cheapest(routes: Point[][], cards: readonly Point[]): Point[] {
  const length = (r: Point[]) =>
    r.reduce(
      (sum, p, i) =>
        i === 0
          ? 0
          : sum + Math.abs(p.x - r[i - 1].x) + Math.abs(p.y - r[i - 1].y),
      0,
    );
  const clear = routes.filter((r) => routeIsClear(r, cards));
  return (clear.length > 0 ? clear : routes).sort(
    (a, b) => length(a) - length(b),
  )[0];
}

/**
 * The line from a member to one it orchestrates: out of the bottom of the upper card, into the top of the
 * lower one. On the next row it runs along the gap between the rows, like an org chart. Further down it goes
 * through the gap between two cards of the rows in between, never behind one. `others` are every other card.
 */
export function orchestrationRoute(
  parent: Point,
  child: Point,
  others: readonly Point[],
): Point[] {
  const from = { x: parent.x + W / 2, y: parent.y + H };
  const to = { x: child.x + W / 2, y: child.y };
  if (to.y <= from.y) return [from, to];
  if (to.y - from.y <= TEAM_ROW_GAP + 1) {
    const bus = (from.y + to.y) / 2;
    return tidy([from, { x: from.x, y: bus }, { x: to.x, y: bus }, to]);
  }
  const top = from.y + HALF_GAP;
  const bottom = to.y - HALF_GAP;
  const between = others.filter((card) => card.y + H > top && card.y < bottom);
  const xs = [from.x, to.x, ...channelsAmong(between)];
  return tidy(
    cheapest(
      xs.map((x) => [
        from,
        { x: from.x, y: top },
        { x, y: top },
        { x, y: bottom },
        { x: to.x, y: bottom },
        to,
      ]),
      others,
    ),
  );
}

/**
 * The line of a pair. Side by side and with nothing between them it is straight. With cards between, it runs
 * over them; on different rows it uses a gap between cards. Cards stacked in one column use the vertical route.
 */
export function pairRoute(
  a: Point,
  b: Point,
  others: readonly Point[],
): Point[] {
  const [left, right] = a.x <= b.x ? [a, b] : [b, a];
  const mid = (card: Point) => card.y + H / 2;
  if (Math.abs(right.x - left.x) < W) {
    const [upper, lower] = a.y <= b.y ? [a, b] : [b, a];
    return orchestrationRoute(upper, lower, others);
  }
  const start = { x: left.x + W, y: mid(left) };
  const end = { x: right.x, y: mid(right) };
  if (Math.abs(left.y - right.y) < 1) {
    const straight = [start, end];
    if (routeIsClear(straight, others)) return straight;
    const lifted = left.y - LIFT;
    return tidy([
      { x: left.x + W / 2, y: left.y },
      { x: left.x + W / 2, y: lifted },
      { x: right.x + W / 2, y: lifted },
      { x: right.x + W / 2, y: right.y },
    ]);
  }
  const span = others.filter(
    (card) =>
      card.y + H > Math.min(left.y, right.y) &&
      card.y < Math.max(left.y, right.y) + H,
  );
  const xs = channelsAmong(span).filter((x) => x > start.x && x < end.x);
  return tidy(
    cheapest(
      [(start.x + end.x) / 2, ...xs].map((x) => [
        start,
        { x, y: start.y },
        { x, y: end.y },
        end,
      ]),
      others,
    ),
  );
}

/** The points as an SVG path, with the corners rounded. */
export function roundedPath(points: readonly Point[], radius = 8): string {
  if (points.length < 2) return "";
  let d = `M ${points[0].x} ${points[0].y}`;
  for (let i = 1; i < points.length - 1; i++) {
    const [before, corner, after] = [points[i - 1], points[i], points[i + 1]];
    const into = Math.hypot(corner.x - before.x, corner.y - before.y);
    const out = Math.hypot(after.x - corner.x, after.y - corner.y);
    const r = Math.min(radius, into / 2, out / 2);
    const enter = {
      x: corner.x + ((before.x - corner.x) / into) * r,
      y: corner.y + ((before.y - corner.y) / into) * r,
    };
    const leave = {
      x: corner.x + ((after.x - corner.x) / out) * r,
      y: corner.y + ((after.y - corner.y) / out) * r,
    };
    d += ` L ${enter.x} ${enter.y} Q ${corner.x} ${corner.y} ${leave.x} ${leave.y}`;
  }
  const last = points[points.length - 1];
  return `${d} L ${last.x} ${last.y}`;
}
