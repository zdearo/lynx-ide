import { Handle, Position, type Node, type NodeProps } from "@xyflow/react";

import { AgentBrandIcon } from "@/components/icons/brands/AgentBrandIcon";
import { ChevronRightGlyph18 } from "@/components/icons/ChevronRightGlyph18";
import { TooltipIconButton } from "@/components/TooltipIconButton";
import { ContextMenu, ContextMenuTrigger } from "@/components/ui/context-menu";
import { cn } from "@/lib/utils";
import type { TeamMember } from "@/team/team-fixtures";
import { TEAM_CARD_HEIGHT, TEAM_CARD_WIDTH } from "@/team/team-layout";
import { TeamMemberContextMenu } from "@/team/TeamMemberMenu";
import { TeamStatusIcon } from "@/team/TeamTree";
import { useTeamStore } from "@/team/team-store";
import { copy } from "@/ui-copy";

export interface TeamCardData extends Record<string, unknown> {
  member: TeamMember;
  hasChildren: boolean;
  expanded: boolean;
  /** Outside the selected member's line of work: drawn faded. */
  dimmed: boolean;
}

export type TeamCardNode = Node<TeamCardData, "member">;

/**
 * One member on the map. The handles are hidden: React Flow needs them to route the lines, and
 * the lines leave from the bottom of a card and arrive at its top (or, for a pair, at its bottom).
 * Right-click (or the menu key) opens the member's actions; the rest is in the sheet.
 */
export function TeamNodeCard({ data, selected }: NodeProps<TeamCardNode>) {
  const { member, hasChildren, expanded, dimmed } = data;
  const status = copy.team.status[member.status];

  return (
    <ContextMenu>
      <ContextMenuTrigger asChild>
        <div
          role="group"
          aria-label={copy.team.rowLabel(member.role, status)}
          data-team-id={member.id}
          tabIndex={0}
          onKeyDown={(event) => {
            // Keys from the chevron belong to it, not to the card.
            if (event.target !== event.currentTarget) return;
            if (event.key === "Enter") {
              useTeamStore.getState().select(member.id);
              useTeamStore.getState().openTerminal(member.id);
            } else if (
              event.key === "ContextMenu" ||
              (event.shiftKey && event.key === "F10")
            ) {
              // Browsers do not send a context-menu event for these keys everywhere, so open it here, at the card's centre.
              event.preventDefault();
              const box = event.currentTarget.getBoundingClientRect();
              event.currentTarget.dispatchEvent(
                new MouseEvent("contextmenu", {
                  bubbles: true,
                  cancelable: true,
                  clientX: box.left + box.width / 2,
                  clientY: box.top + box.height / 2,
                }),
              );
            }
          }}
          style={{ width: TEAM_CARD_WIDTH, height: TEAM_CARD_HEIGHT }}
          className={cn(
            "flex flex-col gap-1.5 rounded-lg border bg-card p-3 text-ui text-card-foreground outline-none focus-visible:ring-2 focus-visible:ring-ring/60",
            selected ? "border-ring" : "border-border",
            dimmed && "opacity-40",
          )}
        >
          <Handle
            id="top"
            type="target"
            position={Position.Top}
            isConnectable={false}
            className="opacity-0!"
          />
          <Handle
            id="bottom"
            type="source"
            position={Position.Bottom}
            isConnectable={false}
            className="opacity-0!"
          />
          <Handle
            id="bottom-in"
            type="target"
            position={Position.Bottom}
            isConnectable={false}
            className="opacity-0!"
          />

          <div className="flex h-5 items-center gap-1.5">
            {hasChildren && (
              <TooltipIconButton
                label={
                  expanded
                    ? copy.team.collapse(member.role)
                    : copy.team.expand(member.role)
                }
                aria-expanded={expanded}
                className="size-5 shrink-0"
                onClick={(event) => {
                  event.stopPropagation();
                  useTeamStore.getState().toggle(member.id);
                }}
              >
                <ChevronRightGlyph18
                  className={cn(
                    "transition-transform",
                    expanded && "rotate-90",
                  )}
                />
              </TooltipIconButton>
            )}
            <AgentBrandIcon
              harness={member.harness}
              className="size-3.5 shrink-0"
            />
            <span className="min-w-0 flex-1 truncate font-medium">
              {member.role}
            </span>
          </div>

          <div className="flex h-4 items-center gap-1.5 text-meta text-muted-foreground">
            <TeamStatusIcon status={member.status} />
            <span className="truncate">{status}</span>
            {member.status === "needsYou" && (
              <span className="ml-auto shrink-0 rounded-sm bg-muted px-1.5 text-attention">
                {copy.team.needsYouBadge}
              </span>
            )}
          </div>

          <p className="truncate text-nested text-muted-foreground">
            {member.task}
          </p>

          <div className="flex h-4 items-center gap-1.5 text-meta tabular-nums">
            {(member.added > 0 || member.removed > 0) && (
              <>
                <span className="text-diff-added">+{member.added}</span>
                <span className="text-diff-removed">−{member.removed}</span>
              </>
            )}
          </div>
        </div>
      </ContextMenuTrigger>
      <TeamMemberContextMenu member={member} />
    </ContextMenu>
  );
}
