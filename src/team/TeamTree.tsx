import { useRef, useState, type KeyboardEvent } from "react";

import type { AgentStatus } from "@/bindings";
import { AgentBrandIcon } from "@/components/icons/brands/AgentBrandIcon";
import { ChevronRightGlyph18 } from "@/components/icons/ChevronRightGlyph18";
import { CircleCheckGlyph18 } from "@/components/icons/CircleCheckGlyph18";
import { CircleQuestionGlyph18 } from "@/components/icons/CircleQuestionGlyph18";
import { DotsGlyph18 } from "@/components/icons/DotsGlyph18";
import { Loader2Glyph18 } from "@/components/icons/Loader2Glyph18";
import { RadioCheckedGlyph18 } from "@/components/icons/RadioCheckedGlyph18";
import { TooltipIconButton } from "@/components/TooltipIconButton";
import {
  DropdownMenu,
  DropdownMenuTrigger,
} from "@/components/ui/dropdown-menu";
import { cn } from "@/lib/utils";
import { TeamMemberMenu } from "@/team/TeamMemberMenu";
import type { TeamMember } from "@/team/team-fixtures";
import { teamCounts, type TeamRow } from "@/team/team-rows";
import { roleOf, useTeamStore } from "@/team/team-store";
import { copy } from "@/ui-copy";

const INDENT_PX = 12;
const BASE_PADDING_PX = 6;

export function TeamStatusIcon({ status }: { status: AgentStatus }) {
  switch (status) {
    case "working":
      return (
        <Loader2Glyph18
          aria-hidden
          className="size-3 shrink-0 animate-spin text-live"
        />
      );
    case "needsYou":
      return (
        <CircleQuestionGlyph18
          aria-hidden
          className="size-3 shrink-0 text-attention"
        />
      );
    case "done":
      return (
        <CircleCheckGlyph18
          aria-hidden
          className="size-3 shrink-0 text-muted-foreground"
        />
      );
    default:
      return (
        <RadioCheckedGlyph18
          aria-hidden
          className="size-3 shrink-0 text-muted-foreground"
        />
      );
  }
}

/** "2 working · 1 need you", for the top bar and the side panel. */
export function TeamSummary({ members }: { members: readonly TeamMember[] }) {
  const { working, needsYou } = teamCounts(members);
  const parts = [
    working > 0 ? copy.team.summary.working(working) : null,
    needsYou > 0 ? copy.team.summary.needsYou(needsYou) : null,
  ].filter((part): part is string => part !== null);
  return (
    <span className="text-meta text-muted-foreground tabular-nums">
      {parts.length > 0 ? parts.join(" · ") : copy.team.summary.quiet}
    </span>
  );
}

/** Moves focus to a member's row once React has drawn it. */
function focusMember(container: HTMLElement | null, id: string) {
  requestAnimationFrame(() =>
    container?.querySelector<HTMLElement>(`[data-team-id="${id}"]`)?.focus(),
  );
}

interface TeamTreeProps {
  rows: TeamRow[];
  members: readonly TeamMember[];
  selectedId: string | null;
}

/** The team as a tree (ARIA treeitems, arrow keys, Enter): the list layout of the team view. */
export function TeamTree({ rows, members, selectedId }: TeamTreeProps) {
  const treeRef = useRef<HTMLDivElement>(null);
  const [menuFor, setMenuFor] = useState<string | null>(null);
  // Exactly one row takes the Tab key: the selected one, else the first.
  const tabStop = rows.some((row) => row.member.id === selectedId)
    ? selectedId
    : (rows[0]?.member.id ?? null);

  const moveTo = (row: TeamRow | undefined) => {
    if (!row) return;
    useTeamStore.getState().select(row.member.id);
    focusMember(treeRef.current, row.member.id);
  };

  function onKeyDown(event: KeyboardEvent<HTMLDivElement>) {
    // Keys from the actions menu come through its portal, where the target is not a row.
    const id =
      event.target instanceof HTMLElement
        ? event.target.dataset.teamId
        : undefined;
    const index = rows.findIndex((row) => row.member.id === id);
    const current = rows[index];
    if (!id || !current) return;
    const store = useTeamStore.getState();

    switch (event.key) {
      case "ArrowDown":
        moveTo(rows[Math.min(rows.length - 1, index + 1)]);
        break;
      case "ArrowUp":
        moveTo(rows[Math.max(0, index - 1)]);
        break;
      case "Home":
        moveTo(rows[0]);
        break;
      case "End":
        moveTo(rows.at(-1));
        break;
      case "ArrowRight":
        if (current.hasChildren && !current.expanded) {
          store.toggle(id);
        } else if (current.hasChildren) {
          const next = rows[index + 1];
          moveTo(next?.depth === current.depth + 1 ? next : undefined);
        }
        break;
      case "ArrowLeft":
        if (current.expanded) {
          store.toggle(id);
        } else {
          // Up to the member that recruited this one.
          moveTo(
            [...rows.slice(0, index)]
              .reverse()
              .find((row) => row.depth === current.depth - 1),
          );
        }
        break;
      case "Enter":
      case " ":
        store.select(id);
        break;
      case "ContextMenu":
        setMenuFor(id);
        break;
      case "F10":
        if (event.shiftKey) setMenuFor(id);
        else return;
        break;
      default:
        return;
    }
    event.preventDefault();
  }

  return (
    <div
      ref={treeRef}
      role="tree"
      aria-label={copy.team.treeLabel}
      onKeyDown={onKeyDown}
      className="flex flex-col gap-px px-1.5 pb-2"
    >
      {rows.map((row) => (
        <TeamRowItem
          key={row.member.id}
          row={row}
          members={members}
          selected={row.member.id === selectedId}
          focusable={row.member.id === tabStop}
          menuOpen={menuFor === row.member.id}
          onMenuOpenChange={(open) => setMenuFor(open ? row.member.id : null)}
        />
      ))}
    </div>
  );
}

interface TeamRowItemProps {
  row: TeamRow;
  members: readonly TeamMember[];
  selected: boolean;
  focusable: boolean;
  menuOpen: boolean;
  onMenuOpenChange: (open: boolean) => void;
}

function TeamRowItem({
  row,
  members,
  selected,
  focusable,
  menuOpen,
  onMenuOpenChange,
}: TeamRowItemProps) {
  const { member, depth, hasChildren, expanded } = row;
  const status = copy.team.status[member.status];
  const pair = roleOf(members, member.pairedWith);
  const waiting = roleOf(members, member.waitingOn);

  return (
    <div
      role="treeitem"
      aria-level={depth + 1}
      aria-expanded={hasChildren ? expanded : undefined}
      aria-selected={selected}
      aria-label={copy.team.rowLabel(member.role, status)}
      data-team-id={member.id}
      tabIndex={focusable ? 0 : -1}
      onClick={() => useTeamStore.getState().select(member.id)}
      onContextMenu={(event) => {
        event.preventDefault();
        useTeamStore.getState().select(member.id);
        onMenuOpenChange(true);
      }}
      style={{ paddingLeft: BASE_PADDING_PX + depth * INDENT_PX }}
      className={cn(
        "group/team flex cursor-default items-start rounded-md py-1 pr-1 text-ui outline-none focus-visible:ring-2 focus-visible:ring-ring/60 focus-visible:ring-inset",
        selected
          ? "bg-sidebar-accent text-sidebar-accent-foreground"
          : "text-sidebar-foreground hover:bg-sidebar-accent/50",
      )}
    >
      <span
        aria-hidden
        className="flex h-5 w-3.5 shrink-0 items-center justify-center"
        onClick={
          hasChildren
            ? (event) => {
                event.stopPropagation();
                useTeamStore.getState().toggle(member.id);
              }
            : undefined
        }
      >
        {hasChildren && (
          <ChevronRightGlyph18
            className={cn(
              "size-3.5 text-muted-foreground transition-transform",
              expanded && "rotate-90",
            )}
          />
        )}
      </span>

      <div className="flex min-w-0 flex-1 flex-col">
        <div className="flex h-5 items-center gap-1.5">
          <TeamStatusIcon status={member.status} />
          <AgentBrandIcon
            harness={member.harness}
            className="size-3 shrink-0"
          />
          <span className="min-w-0 truncate font-medium">{member.role}</span>
          <span className="shrink-0 text-meta text-muted-foreground">
            {status}
          </span>
          <span className="min-w-0 flex-1" />
          {(member.added > 0 || member.removed > 0) && (
            <span className="shrink-0 text-meta tabular-nums">
              <span className="text-diff-added">+{member.added}</span>{" "}
              <span className="text-diff-removed">−{member.removed}</span>
            </span>
          )}

          <DropdownMenu open={menuOpen} onOpenChange={onMenuOpenChange}>
            <DropdownMenuTrigger asChild>
              <TooltipIconButton
                label={copy.team.actions(member.role)}
                tabIndex={-1}
                className={cn(
                  "size-5 shrink-0 opacity-0 transition-opacity group-hover/team:opacity-100 group-focus-within/team:opacity-100",
                  menuOpen && "opacity-100",
                )}
                onClick={(event) => event.stopPropagation()}
              >
                <DotsGlyph18 />
              </TooltipIconButton>
            </DropdownMenuTrigger>
            <TeamMemberMenu member={member} />
          </DropdownMenu>
        </div>

        <p className="truncate text-nested text-muted-foreground">
          {member.task}
          {member.branch && (
            <span className="font-mono"> · {member.branch}</span>
          )}
        </p>
        {(waiting || pair || member.note || member.question) && (
          <p className="flex flex-wrap items-center gap-1 pt-0.5 text-meta text-muted-foreground">
            {waiting && <span>{copy.team.waitingOn(waiting)}</span>}
            {pair && (
              <span className="rounded-sm bg-muted px-1.5">
                {copy.team.pairedWith(pair)}
              </span>
            )}
            {member.note && (
              <span className="rounded-sm bg-muted px-1.5">{member.note}</span>
            )}
            {member.question && (
              <span className="text-attention">{member.question}</span>
            )}
          </p>
        )}
      </div>
    </div>
  );
}
