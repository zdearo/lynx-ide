import { useEffect, useRef } from "react";

import { BotAvatar } from "@/bots/BotAvatar";
import { useBotsStore } from "@/bots/bots-store";
import { AgentBrandIcon } from "@/components/icons/brands/AgentBrandIcon";
import {
  CaretRightIcon,
  TerminalWindowIcon,
  XIcon,
} from "@/components/icons/line-icons";
import { TooltipIconButton } from "@/components/TooltipIconButton";
import { ContextMenu, ContextMenuTrigger } from "@/components/ui/context-menu";
import { useNow } from "@/hooks/use-now";
import { cn } from "@/lib/utils";
import type { TeamMember } from "@/team/team-fixtures";
import { hasTask } from "@/team/team-hierarchy";
import { runningLabelOf } from "@/team/team-live";
import { TeamMemberContextMenu } from "@/team/TeamMemberMenu";
import { TeamNodeBody } from "@/team/TeamNodeBody";
import { TeamStatusIcon } from "@/team/TeamStatus";
import { botOf, engineOf, useTeamStore } from "@/team/team-store";
import { useTerminalStore } from "@/terminal/terminal-store";
import { copy } from "@/ui-copy";

/**
 * One member on the map. A click picks it, a double click (or Enter) opens its terminal in the card, and the right
 * click (or the menu key) opens the member's actions. In terminal mode a card carries a button for its terminal.
 * An expanded card shows its terminal in place of its summary.
 */
export function TeamNodeCard({
  member,
  hasChildren,
  expanded,
  open,
  selected,
  dimmed,
  canOpen,
  terminalVisible,
  onOpen,
  onClose,
}: {
  member: TeamMember;
  hasChildren: boolean;
  /** Whether its own members are listed under it. */
  expanded: boolean;
  /** Whether its terminal is open in the card. */
  open: boolean;
  selected: boolean;
  dimmed: boolean;
  /** Whether the card offers its terminal: a card whose terminal is not open on the map. */
  canOpen: boolean;
  /** Whether the open card's terminal is on the screen; off it, the terminal is not mounted. */
  terminalVisible: boolean;
  onOpen: () => void;
  onClose: () => void;
}) {
  const rootRef = useRef<HTMLDivElement>(null);
  // Closing the terminal leaves the focus on the card, which stays where it is.
  const close = () => {
    onClose();
    rootRef.current?.focus();
  };
  // A card whose terminal opens takes the focus, unless the terminal takes it itself (see TerminalView).
  useEffect(() => {
    if (open && !rootRef.current?.contains(document.activeElement)) {
      rootRef.current?.focus();
    }
  }, [open]);
  const status = copy.team.status[member.status];
  const bots = useBotsStore((state) => state.bots);
  const bot = botOf(member, bots);
  const now = useNow();
  const running = useTerminalStore((state) =>
    runningLabelOf(member, state.sessions, now),
  );

  const title = (
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
            void useTeamStore.getState().toggle(member.id);
          }}
        >
          <CaretRightIcon
            className={cn("transition-transform", expanded && "rotate-90")}
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
      <span className="min-w-0 flex-1 truncate font-medium">{member.role}</span>
      {/* Capped at 60% of the row: a long model name is cut, and the role keeps the rest. */}
      <span className="min-w-0 max-w-[60%] truncate text-meta text-muted-foreground">
        {engineOf(member, bots)}
      </span>
    </div>
  );

  return (
    <ContextMenu>
      <ContextMenuTrigger asChild>
        <div
          role="group"
          aria-label={copy.team.rowLabel(member.role, status, running)}
          ref={rootRef}
          data-team-id={member.id}
          data-selected={selected ? "true" : undefined}
          tabIndex={0}
          onClick={() => useTeamStore.getState().select(member.id)}
          onDoubleClick={onOpen}
          onKeyDownCapture={(event) => {
            // Shift+Escape closes the terminal from anywhere in the card, the terminal included: it is taken before the
            // terminal reads the key.
            if (open && event.key === "Escape" && event.shiftKey) {
              event.preventDefault();
              event.stopPropagation();
              close();
            }
          }}
          onKeyDown={(event) => {
            // Escape belongs to the agent inside its terminal; anywhere else it closes the terminal.
            if (
              open &&
              event.key === "Escape" &&
              !(event.target as Element).closest(".xterm")
            ) {
              event.preventDefault();
              // The team clears the pick on Escape; here Escape only closes the terminal.
              event.stopPropagation();
              close();
              return;
            }
            // Keys from the chevron belong to it, not to the card.
            if (event.target !== event.currentTarget) return;
            if (event.key === "Enter") {
              useTeamStore.getState().select(member.id);
              onOpen();
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
          className={cn(
            "group size-full flex flex-col gap-1.5 overflow-hidden rounded-lg border bg-card p-3 text-ui text-card-foreground outline-none focus-visible:ring-2 focus-visible:ring-ring/60",
            selected ? "border-ring" : "border-border",
            dimmed && "opacity-80",
            open && "gap-0 p-0",
          )}
        >
          {open ? (
            <>
              <div className="flex h-9 shrink-0 items-center gap-2 border-b px-3">
                {title}
                <TooltipIconButton
                  label={
                    member.kind === "bot"
                      ? copy.team.closeChat
                      : copy.team.closeTerminal
                  }
                  className="ml-auto shrink-0"
                  onClick={close}
                >
                  <XIcon />
                </TooltipIconButton>
              </div>
              {/* A press in the body is for the terminal or the chat, not a drag of the card. */}
              <div
                className="flex min-h-0 flex-1 flex-col"
                onPointerDown={(event) => event.stopPropagation()}
              >
                <TeamNodeBody
                  member={member}
                  terminalVisible={terminalVisible}
                />
              </div>
            </>
          ) : (
            <>
              {title}

              <div className="flex h-4 items-center gap-1.5 text-meta text-muted-foreground">
                <TeamStatusIcon status={member.status} />
                <span className="truncate">{status}</span>
                {(member.status === "needsYou" || running !== null) && (
                  <span className="ml-auto flex shrink-0 items-center gap-1.5">
                    {member.status === "needsYou" && (
                      <span className="rounded-sm bg-muted px-1.5 text-attention">
                        {copy.team.needsYouBadge}
                      </span>
                    )}
                    {running !== null && <span>{running}</span>}
                  </span>
                )}
              </div>

              <p className="truncate text-nested text-muted-foreground">
                {hasTask(member) ? member.task : copy.team.noTask}
              </p>

              <div className="mt-auto flex h-6 items-center gap-1.5 text-meta tabular-nums">
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
                {canOpen && (
                  <TooltipIconButton
                    label={copy.team.openTerminal}
                    className="ml-auto size-6 shrink-0 text-muted-foreground"
                    onClick={(event) => {
                      event.stopPropagation();
                      onOpen();
                    }}
                  >
                    <TerminalWindowIcon />
                  </TooltipIconButton>
                )}
              </div>
            </>
          )}
        </div>
      </ContextMenuTrigger>
      <TeamMemberContextMenu member={member} hasChildren={hasChildren} />
    </ContextMenu>
  );
}
