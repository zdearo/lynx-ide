import type { Node } from "@xyflow/react";

import { TEAM_CARD_HEIGHT, TEAM_CARD_WIDTH } from "@/team/team-layout";

export type TeamGhostNode = Node<Record<string, never>, "ghost">;

/** Where a card held with the mouse will land when it is let go. */
export function TeamGhostCard() {
  return (
    <div
      aria-hidden
      style={{ width: TEAM_CARD_WIDTH, height: TEAM_CARD_HEIGHT }}
      className="rounded-lg border-2 border-dashed border-ring/70 bg-ring/10"
    />
  );
}
