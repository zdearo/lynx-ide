import type { TeamMember } from "@/team/team-fixtures";

interface Connection {
  source: string;
  target: string;
  sourceHandle?: string | null;
  targetHandle?: string | null;
}

/**
 * Only a member with an orchestrator above it has a task and an acceptance line. A member without one only
 * receives conversation and answers it.
 */
export const hasTask = (member: TeamMember): boolean =>
  member.parentId !== null;

/**
 * Whether a member is drawn on the map: every orchestrator above it is open. A member whose orchestrator is not in the
 * team is not drawn either, as the layout leaves it out.
 */
export function isDrawn(
  members: readonly Pick<TeamMember, "id" | "parentId">[],
  expanded: readonly string[],
  id: string,
): boolean {
  const parentOf = new Map(
    members.map((member) => [member.id, member.parentId]),
  );
  if (!parentOf.has(id)) return false;
  const seen = new Set<string>();
  let above = parentOf.get(id) ?? null;
  while (above !== null) {
    if (!parentOf.has(above) || !expanded.includes(above) || seen.has(above)) {
      return false;
    }
    seen.add(above);
    above = parentOf.get(above) ?? null;
  }
  return true;
}

/** What a line drawn between two handles means: the top and bottom handles orchestrate, the sides talk. */
export type TeamLink =
  | { kind: "orchestrates"; parent: string; child: string }
  | { kind: "talks"; from: string; to: string };

const isSide = (handle: string | null | undefined) =>
  handle === "left" || handle === "right";

/** Top to bottom is an orchestration (from either end), side to side is a pair; anything else is no line. */
export function linkOf(connection: Connection): TeamLink | null {
  const { source, target, sourceHandle, targetHandle } = connection;
  if (isSide(sourceHandle) && isSide(targetHandle)) {
    return { kind: "talks", from: source, to: target };
  }
  if (sourceHandle === "bottom" && targetHandle === "top") {
    return { kind: "orchestrates", parent: source, child: target };
  }
  if (sourceHandle === "top" && targetHandle === "bottom") {
    return { kind: "orchestrates", parent: target, child: source };
  }
  return null;
}

/**
 * Whether `parentId` can orchestrate `childId`: it is another member of the same worktree's team, it is an
 * orchestrator, and it is not under the child (a member cannot answer to its own member). A bot is never a child:
 * it only orchestrates.
 */
export function canOrchestrate(
  members: readonly TeamMember[],
  parentId: string,
  childId: string,
): boolean {
  const byId = new Map(members.map((member) => [member.id, member]));
  if (parentId === childId || !byId.has(childId)) return false;
  if (byId.get(childId)?.kind === "bot") return false;
  if (!byId.get(parentId)?.orchestrator) return false;
  if (byId.get(parentId)?.worktree !== byId.get(childId)?.worktree)
    return false;
  const seen = new Set<string>();
  for (
    let above = byId.get(parentId)?.parentId;
    above && !seen.has(above);
    above = byId.get(above)?.parentId
  ) {
    if (above === childId) return false;
    seen.add(above);
  }
  return true;
}

/** The orchestrators a member could answer to, with whether it does now and whether it can. */
export function orchestratorOptionsOf(
  id: string,
  members: readonly TeamMember[],
) {
  const parentId = members.find((member) => member.id === id)?.parentId;
  return members
    .filter((member) => member.orchestrator && member.id !== id)
    .map((member) => ({
      member,
      current: member.id === parentId,
      enabled: member.id === parentId || canOrchestrate(members, member.id, id),
    }));
}
