import type { TeamMember } from "@/team/team-fixtures";
import { partnersOf, type TeamPair } from "@/team/team-pairs";

/** Card size in flow pixels. The cards are drawn at this size, so the layout places them without measuring. */
export const TEAM_CARD_WIDTH = 224;
export const TEAM_CARD_HEIGHT = 112;
const GAP_X = 32;
const GAP_Y = 64;
/** The gap between two rows of cards: the lines between rows run in it. */
export const TEAM_ROW_GAP = GAP_Y;
/** From one row to the next: a card and the gap under it. */
const ROW_HEIGHT = TEAM_CARD_HEIGHT + GAP_Y;
/** Between the right edge of one tree's block and the left edge of the next: a card gap plus 64 px, so the lines of two trees never meet. */
export const TEAM_TREE_GAP = GAP_X + 64;

export interface TeamPosition {
  x: number;
  y: number;
}

/** The key of the members that answer to no one, in `SiblingOrder`. */
export const TOP_LEVEL = "";

/** The left-to-right order of each member's own members, by the member's id. A member not listed goes last. */
export type SiblingOrder = Readonly<Record<string, readonly string[]>>;

function inSavedOrder(
  group: readonly TeamMember[],
  saved: readonly string[] = [],
): TeamMember[] {
  const rank = (member: TeamMember) => {
    const at = saved.indexOf(member.id);
    return at === -1 ? saved.length : at;
  };
  return [...group].sort((a, b) => rank(a) - rank(b));
}

/** From one card to the next on a row, when they sit side by side. */
const STEP = TEAM_CARD_WIDTH + GAP_X;

/**
 * Centres for one row of cards, from the centres they want (in order, left to right): cards that do not fit
 * are pushed apart as little as possible, so a row with a gap closes up and a crowded one spreads evenly.
 * It is a least-squares fit with a fixed minimum distance, solved by pooling the cards that push each other.
 */
function packRow(wanted: readonly number[]): number[] {
  const pools: { sum: number; count: number }[] = [];
  wanted.forEach((centre, index) => {
    pools.push({ sum: centre - index * STEP, count: 1 });
    while (pools.length > 1) {
      const top = pools[pools.length - 1];
      const below = pools[pools.length - 2];
      if (below.sum / below.count <= top.sum / top.count) break;
      below.sum += top.sum;
      below.count += top.count;
      pools.pop();
    }
  });
  const packed: number[] = [];
  for (const pool of pools) {
    for (let i = 0; i < pool.count; i++) {
      packed.push(pool.sum / pool.count + packed.length * STEP);
    }
  }
  return packed;
}

/** One tree of the team: a root orchestrator with the members under it, or the members that answer to no one and have no partner in a tree. */
interface Block {
  /** The root's id. Null for the last block, which has no root. */
  root: string | null;
  /** The members by row, the root's row being 0. */
  rows: Map<number, TeamMember[]>;
}

/**
 * The centre of each member of one block, by id. Each row is laid out on its own: a member wants to be centred under its
 * parent, and members that want the same place are spread around it, in the order of `readingOrder`. The root ends on 0.
 */
function packBlock(
  block: Block,
  order: SiblingOrder,
  pairs: readonly TeamPair[],
  readingOrder: ReadonlyMap<string, number>,
): Map<string, number> {
  const centres = new Map<string, number>();
  for (const row of [...block.rows.keys()].sort((a, b) => a - b)) {
    const inRow = block.rows.get(row) ?? [];
    // A parent is always on an earlier row, so its centre is known by now.
    const wanted = new Map<string, number>();
    const siblings = new Map<string | null, TeamMember[]>();
    for (const member of inRow) {
      siblings.set(member.parentId, [
        ...(siblings.get(member.parentId) ?? []),
        member,
      ]);
    }
    for (const [parentId, group] of siblings) {
      const middle = parentId === null ? 0 : (centres.get(parentId) ?? 0);
      group.forEach((member, index) => {
        wanted.set(member.id, middle + (index - (group.length - 1) / 2) * STEP);
      });
    }
    // A member with no orchestrator wants the place of its partner, so the pair sits side by side: on its
    // right unless the saved order puts it first.
    for (const member of inRow) {
      if (member.parentId !== null || member.orchestrator) continue;
      const partner = partnersOf(pairs, member.id).find(
        (id) => centres.has(id) || wanted.has(id),
      );
      if (partner === undefined) continue;
      const saved = order[TOP_LEVEL] ?? [];
      const [mine, theirs] = [saved.indexOf(member.id), saved.indexOf(partner)];
      const side = mine !== -1 && theirs !== -1 && mine < theirs ? -1 : 1;
      wanted.set(
        member.id,
        (centres.get(partner) ?? wanted.get(partner) ?? 0) + side,
      );
    }
    const ordered = [...inRow].sort(
      (a, b) =>
        (wanted.get(a.id) ?? 0) - (wanted.get(b.id) ?? 0) ||
        (readingOrder.get(a.id) ?? 0) - (readingOrder.get(b.id) ?? 0),
    );
    const packed = packRow(ordered.map((member) => wanted.get(member.id) ?? 0));
    ordered.forEach((member, index) => centres.set(member.id, packed[index]));
  }
  // The root's partner may sit on its row and pull the root off 0: the whole block moves so the root is on 0.
  const shift = block.root ? -(centres.get(block.root) ?? 0) : 0;
  for (const [id, centre] of centres) centres.set(id, centre + shift);
  return centres;
}

/**
 * Where each member on screen goes in the org chart. The team is laid out tree by tree: a root orchestrator with
 * the members under it is one block, its root centred on x = 0 in the block. A member sits one row under its parent,
 * plus its extra rows (never fewer than zero), so moving a member down moves its whole branch. A member with no
 * orchestrator joins the block of its first partner, on that partner's row; one with no partner forms the last
 * block, one row below the top. The blocks go left to right in the order of `order` (the top level), then by
 * reading order, with `TEAM_TREE_GAP` between two blocks, so no two trees overlap. The first block stays where its
 * own layout puts it: the first tree's root is on x = 0, so the map is not shifted. A collapsed member's own
 * members are left out, and so is a member whose parent is not in the team.
 */
export function teamLayout(
  members: readonly TeamMember[],
  expanded: readonly string[],
  extraRows: Readonly<Record<string, number>>,
  order: SiblingOrder = {},
  pairs: readonly TeamPair[] = [],
): Map<string, TeamPosition> {
  const childrenOf = new Map<string, TeamMember[]>();
  for (const member of members) {
    if (member.parentId === null) continue;
    childrenOf.set(member.parentId, [
      ...(childrenOf.get(member.parentId) ?? []),
      member,
    ]);
  }

  for (const [parentId, group] of childrenOf) {
    childrenOf.set(parentId, inSavedOrder(group, order[parentId]));
  }

  // Only the members reached from a root are placed, so a cycle outside the team is never walked.
  const readingOrder = new Map<string, number>();
  const blockOf = new Map<string, Block>();
  const rowOf = new Map<string, number>();
  function place(block: Block, member: TeamMember, row: number) {
    readingOrder.set(member.id, readingOrder.size);
    blockOf.set(member.id, block);
    rowOf.set(member.id, row);
    block.rows.set(row, [...(block.rows.get(row) ?? []), member]);
  }
  function visit(block: Block, member: TeamMember, row: number) {
    place(block, member, row);
    if (!expanded.includes(member.id)) return;
    for (const child of childrenOf.get(member.id) ?? []) {
      visit(block, child, row + 1 + Math.max(0, extraRows[child.id] ?? 0));
    }
  }

  const roots = members.filter((member) => member.parentId === null);
  const trees: Block[] = [];
  for (const root of inSavedOrder(
    roots.filter((member) => member.orchestrator),
    order[TOP_LEVEL],
  )) {
    const block: Block = { root: root.id, rows: new Map() };
    trees.push(block);
    visit(block, root, 0);
  }
  const alone: Block = { root: null, rows: new Map() };
  for (const loose of roots.filter((member) => !member.orchestrator)) {
    const home = partnersOf(pairs, loose.id).flatMap((id) => {
      const block = blockOf.get(id);
      const row = rowOf.get(id);
      return block && row !== undefined ? [{ block, row }] : [];
    })[0];
    if (home) place(home.block, loose, home.row);
    else place(alone, loose, 1);
  }

  const blocks = [...trees, alone].filter((block) => block.rows.size > 0);
  const positions = new Map<string, TeamPosition>();
  let rightEdge = 0;
  blocks.forEach((block, index) => {
    const centres = packBlock(block, order, pairs, readingOrder);
    // Whole pixels, so the gap between two blocks is exact and not rounded away.
    const lefts = new Map<string, { x: number; y: number }>();
    for (const [id, centre] of centres) {
      const row = rowOf.get(id) ?? 0;
      lefts.set(id, {
        x: Math.round(centre - TEAM_CARD_WIDTH / 2),
        y: row * ROW_HEIGHT,
      });
    }
    const xs = [...lefts.values()].map((spot) => spot.x);
    const left = Math.min(...xs);
    const offset = index === 0 ? 0 : rightEdge + TEAM_TREE_GAP - left;
    for (const [id, spot] of lefts) {
      positions.set(id, { x: spot.x + offset, y: spot.y });
    }
    rightEdge = offset + Math.max(...xs) + TEAM_CARD_WIDTH;
  });
  return positions;
}

/**
 * The extra rows that put a member's card on the row nearest to y, under its parent. Never above the
 * row right under the parent, so the result is at least zero.
 */
export function extraRowsAt(
  members: readonly TeamMember[],
  positions: ReadonlyMap<string, TeamPosition>,
  id: string,
  y: number,
): number {
  const parentId = members.find((member) => member.id === id)?.parentId;
  // A member that answers to no one stays on the top row.
  if (!parentId) return 0;
  const parent = positions.get(parentId);
  const parentRow = parent ? Math.round(parent.y / ROW_HEIGHT) : 0;
  return Math.max(0, Math.round(y / ROW_HEIGHT) - parentRow - 1);
}

/**
 * The order of a member's siblings once it is dropped with its left edge at x: among the siblings on the
 * row it is on in `positions`, it goes after the ones to its left. Returns `order` itself when nothing changes.
 */
export function siblingOrderAt(
  members: readonly TeamMember[],
  positions: ReadonlyMap<string, TeamPosition>,
  id: string,
  x: number,
  order: SiblingOrder,
  pairs: readonly TeamPair[] = [],
): SiblingOrder {
  const self = members.find((member) => member.id === id);
  const row = positions.get(id)?.y;
  if (!self || row === undefined) return order;
  const key = self.parentId ?? TOP_LEVEL;
  if (self.parentId === null && !self.orchestrator) {
    // No orchestrator: it only picks a side of the partner it sits beside.
    const partner = partnersOf(pairs, id).find(
      (other) => positions.get(other)?.y === row,
    );
    if (partner === undefined) return order;
    const first = x < (positions.get(partner)?.x ?? 0);
    const rest = (order[key] ?? []).filter(
      (other) => other !== id && other !== partner,
    );
    const next = first ? [id, partner, ...rest] : [partner, id, ...rest];
    // Nothing saved means its default side, the right.
    const same = order[key] ? order[key].join() === next.join() : !first;
    return same ? order : { ...order, [key]: next };
  }
  const siblings = inSavedOrder(
    members.filter((member) => member.parentId === self.parentId),
    order[key],
  ).map((member) => member.id);
  const others = siblings.filter((other) => other !== id);
  const sameRow = others.filter((other) => positions.get(other)?.y === row);
  if (sameRow.length === 0) return order;
  const left = sameRow.filter((other) => (positions.get(other)?.x ?? 0) < x);
  const anchor = left.length === 0 ? sameRow[0] : left[left.length - 1];
  const next = [...others];
  next.splice(next.indexOf(anchor) + (left.length === 0 ? 0 : 1), 0, id);
  if (next.every((other, index) => other === siblings[index])) return order;
  return { ...order, [key]: next };
}
