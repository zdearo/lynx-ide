import { Handle, Position, type Node, type NodeProps } from "@xyflow/react";

import { BotAvatar } from "@/bots/BotAvatar";
import { useBotsStore } from "@/bots/bots-store";
import { AgentBrandIcon } from "@/components/icons/brands/AgentBrandIcon";
import { ChevronRightGlyph18 } from "@/components/icons/ChevronRightGlyph18";
import { TooltipIconButton } from "@/components/TooltipIconButton";
import { ContextMenu, ContextMenuTrigger } from "@/components/ui/context-menu";
import { cn } from "@/lib/utils";
import type { TeamMember } from "@/team/team-fixtures";
import { hasTask } from "@/team/team-hierarchy";
import { TEAM_CARD_HEIGHT, TEAM_CARD_WIDTH } from "@/team/team-layout";
import { TeamMemberContextMenu } from "@/team/TeamMemberMenu";
import { TeamStatusIcon } from "@/team/TeamStatus";
import { botOf, engineOf, openMember, useTeamStore } from "@/team/team-store";
import { copy } from "@/ui-copy";

export interface TeamCardData extends Record<string, unknown> {
  member: TeamMember;
  hasChildren: boolean;
  expanded: boolean;
  /** Outside the selected member's line of work: drawn faded. */
  dimmed: boolean;
}

export type TeamCardNode = Node<TeamCardData, "member">;

// The handles start a line: the top and bottom ones orchestrate, the side ones make a pair. They show with the mouse over the card or the focus inside it.
const HANDLE =
  "size-2! border-ring! bg-background! opacity-0 transition-opacity group-focus-within:opacity-100 group-hover:opacity-100";

/**
 * One member on the map. Its top handle takes an orchestrator's line (it answers to it), its bottom handle,
 * only there with orchestrator mode on, gives one (it orchestrates the other). The side handles make pairs.
 * Right-click (or the menu key) opens the member's actions; the rest is in the sheet.
 */
export function TeamNodeCard({ data, selected }: NodeProps<TeamCardNode>) {
  const { member, hasChildren, expanded, dimmed } = data;
  const status = copy.team.status[member.status];
  const bots = useBotsStore((state) => state.bots);
  const bot = botOf(member, bots);

  return (
    <ContextMenu>
      <ContextMenuTrigger asChild>
        <div
          role="group"
          aria-label={copy.team.rowLabel(member.role, status)}
          data-team-id={member.id}
          tabIndex={0}
          onKeyDown={(event) => {
            // React Flow would move a selected card with the arrows; here they belong to the card.
            if (event.key.startsWith("Arrow")) event.stopPropagation();
            // Keys from the chevron belong to it, not to the card.
            if (event.target !== event.currentTarget) return;
            if (event.key === "Enter") {
              useTeamStore.getState().select(member.id);
              openMember(member.id);
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
            } else if (
              event.altKey &&
              member.parentId !== null &&
              (event.key === "ArrowDown" || event.key === "ArrowUp")
            ) {
              // Alt+Down moves the card one row down, Alt+Up one row up; it never leaves the row under its parent.
              event.preventDefault();
              const store = useTeamStore.getState();
              store.setExtraRows(
                member.id,
                (store.extraRows[member.id] ?? 0) +
                  (event.key === "ArrowDown" ? 1 : -1),
              );
            }
          }}
          style={{ width: TEAM_CARD_WIDTH, height: TEAM_CARD_HEIGHT }}
          className={cn(
            "group flex flex-col gap-1.5 rounded-lg border bg-card p-3 text-ui text-card-foreground outline-none focus-visible:ring-2 focus-visible:ring-ring/60",
            selected ? "border-ring" : "border-border",
            dimmed && "opacity-80",
          )}
        >
          {/* A bot is never a child, so no line comes in from above it. */}
          {member.kind === "agent" && (
            <Handle
              id="top"
              type="target"
              position={Position.Top}
              className={HANDLE}
            />
          )}
          {member.orchestrator && (
            <Handle
              id="bottom"
              type="source"
              position={Position.Bottom}
              className={HANDLE}
            />
          )}
          <Handle
            id="left"
            type="target"
            position={Position.Left}
            className={HANDLE}
          />
          <Handle
            id="right"
            type="source"
            position={Position.Right}
            className={HANDLE}
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
                className="nodrag size-5 shrink-0"
                onClick={(event) => {
                  event.stopPropagation();
                  void useTeamStore.getState().toggle(member.id);
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
            {member.kind === "bot" ? (
              <BotAvatar
                name={member.role}
                color={bot?.color ?? "graphite"}
                size="xs"
              />
            ) : (
              <AgentBrandIcon
                harness={member.harness}
                className="size-3.5 shrink-0"
              />
            )}
            <span className="min-w-0 flex-1 truncate font-medium">
              {member.role}
            </span>
            {/* Capped at 60% of the row: a long model name is cut, and the role keeps the rest. */}
            <span className="min-w-0 max-w-[60%] truncate text-meta text-muted-foreground">
              {engineOf(member, bots)}
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
            {hasTask(member) ? member.task : copy.team.noTask}
          </p>

          <div className="flex h-4 items-center gap-1.5 text-meta tabular-nums">
            {(member.added > 0 || member.removed > 0) && (
              <>
                <span className="text-diff-added">+{member.added}</span>
                <span className="text-diff-removed">−{member.removed}</span>
              </>
            )}
            {member.orchestrator && (
              <span className="ml-auto text-muted-foreground">
                {copy.team.orchestratorBadge}
              </span>
            )}
          </div>
        </div>
      </ContextMenuTrigger>
      <TeamMemberContextMenu member={member} />
    </ContextMenu>
  );
}
