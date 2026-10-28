import type { TeamMember } from "@/team/team-fixtures";

/** A permission for two members to talk, drawn as a dashed line between their cards. */
export interface TeamPair {
  from: string;
  to: string;
}

/** One key for both directions, so a pair is one line however it was drawn. */
export function pairKey(a: string, b: string): string {
  return [a, b].sort().join("|");
}

export function isLinked(
  pairs: readonly TeamPair[],
  a: string,
  b: string,
): boolean {
  const key = pairKey(a, b);
  return pairs.some((pair) => pairKey(pair.from, pair.to) === key);
}

/** The members a member is paired with, by id. */
export function partnersOf(pairs: readonly TeamPair[], id: string): string[] {
  return pairs.flatMap((pair) => {
    if (pair.from === id) return [pair.to];
    if (pair.to === id) return [pair.from];
    return [];
  });
}

/**
 * A new pair joins two different members that are not parent and child, and are not paired yet.
 * Parent and child already talk through the tree, so a pair between them would be a second line.
 * Two bots do not pair: a bot takes messages from agents, and only orchestrates.
 */
export function isValidConnection(
  connection: { source: string; target: string },
  members: readonly TeamMember[],
  pairs: readonly TeamPair[],
): boolean {
  const { source, target } = connection;
  if (source === target || isLinked(pairs, source, target)) return false;
  const find = (id: string) => members.find((member) => member.id === id);
  // Two runs are independent: a pair stays inside one.
  if (find(source)?.runId !== find(target)?.runId) return false;
  if (find(source)?.kind === "bot" && find(target)?.kind === "bot") {
    return false;
  }
  return find(source)?.parentId !== target && find(target)?.parentId !== source;
}

/** Every other member, with whether it is paired with this one and whether it can be. */
export function linkOptionsOf(
  id: string,
  members: readonly TeamMember[],
  pairs: readonly TeamPair[],
) {
  return members
    .filter((member) => member.id !== id)
    .map((member) => {
      const linked = isLinked(pairs, id, member.id);
      return {
        member,
        linked,
        enabled:
          linked ||
          isValidConnection({ source: id, target: member.id }, members, pairs),
      };
    });
}
