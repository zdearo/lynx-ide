import { useEffect, useRef, useState } from "react";

import { AgentBrandIcon } from "@/components/icons/brands/AgentBrandIcon";
import { DotsGlyph18 } from "@/components/icons/DotsGlyph18";
import { TooltipIconButton } from "@/components/TooltipIconButton";
import { Button } from "@/components/ui/button";
import {
  DropdownMenu,
  DropdownMenuTrigger,
} from "@/components/ui/dropdown-menu";
import { DOCK_RIGHT_QUERY } from "@/hooks/layout-queries";
import { useMediaQuery } from "@/hooks/use-media-query";
import { useRightPanelStore } from "@/right-panel/right-panel-store";
import { formatAge } from "@/terminal/relative-time";
import type { TeamMember } from "@/team/team-fixtures";
import { focusMemberCard } from "@/team/team-focus";
import { TeamMemberMenu } from "@/team/TeamMemberMenu";
import { TeamQuestion } from "@/team/TeamQuestion";
import { TeamStatusIcon } from "@/team/TeamTree";
import { agentNameOf, roleOf, useTeamStore } from "@/team/team-store";
import { copy } from "@/ui-copy";

/**
 * The picked member, in a panel over the team. It is not modal: the team stays in front of it and
 * another member can be picked from it. Escape clears the pick (see TeamView).
 */
export function TeamNodeSheet({
  member,
  members,
}: {
  member: TeamMember;
  members: readonly TeamMember[];
}) {
  const ref = useRef<HTMLDivElement>(null);
  const canDock = useMediaQuery(DOCK_RIGHT_QUERY);
  const agent = agentNameOf(member.harness);
  const dependsOn = roleOf(members, member.waitingOn);
  const talksTo = roleOf(members, member.pairedWith);
  const changed = member.added > 0 || member.removed > 0;
  // Ages are read when the panel opens; it is remounted for each member picked.
  const [openedAt] = useState(() => Date.now());

  // Focus goes into the panel when it opens, and back to the member's card when it closes.
  useEffect(() => {
    ref.current?.focus();
    return () => focusMemberCard(member.id);
  }, [member.id]);

  return (
    <div
      ref={ref}
      role="dialog"
      aria-label={copy.team.sheetLabel(member.role, agent)}
      tabIndex={-1}
      className="absolute top-3 right-3 flex max-h-[calc(100%-1.5rem)] w-80 max-w-[calc(100%-1.5rem)] flex-col gap-3 overflow-y-auto rounded-xl bg-popover p-3 text-ui text-popover-foreground shadow-md ring-1 ring-foreground/10 outline-none"
    >
      <header className="flex min-w-0 items-center gap-2">
        <AgentBrandIcon harness={member.harness} className="size-4 shrink-0" />
        <h3 className="min-w-0 flex-1 truncate font-medium">{member.role}</h3>
        <span className="shrink-0 text-meta text-muted-foreground">
          {agent}
        </span>
      </header>
      <p className="flex items-center gap-1.5 text-meta text-muted-foreground">
        <TeamStatusIcon status={member.status} />
        {copy.team.status[member.status]}
      </p>

      <dl className="grid grid-cols-[auto_minmax(0,1fr)] gap-x-3 gap-y-2">
        <dt className="text-muted-foreground">{copy.team.task}</dt>
        <dd className="min-w-0">{member.task}</dd>
        <dt className="text-muted-foreground">{copy.team.acceptance}</dt>
        <dd className="min-w-0">{member.acceptance}</dd>
      </dl>

      {member.question && (
        <TeamQuestion
          id={member.id}
          role={member.role}
          question={member.question}
        />
      )}

      <dl className="grid grid-cols-[auto_minmax(0,1fr)] items-baseline gap-x-3 gap-y-2">
        {dependsOn && (
          <>
            <dt className="text-muted-foreground">{copy.team.dependsOn}</dt>
            <dd className="min-w-0">
              <button
                type="button"
                className="max-w-full truncate text-left underline-offset-3 hover:underline"
                onClick={() => useTeamStore.getState().select(member.waitingOn)}
              >
                {dependsOn}
              </button>
            </dd>
          </>
        )}
        {talksTo && (
          <>
            <dt className="text-muted-foreground">{copy.team.talksTo}</dt>
            <dd className="min-w-0">
              <button
                type="button"
                className="max-w-full truncate text-left underline-offset-3 hover:underline"
                onClick={() =>
                  useTeamStore.getState().select(member.pairedWith)
                }
              >
                {talksTo}
              </button>
            </dd>
          </>
        )}
        {member.note && (
          <>
            <dt className="text-muted-foreground">{copy.team.notes}</dt>
            <dd className="min-w-0">
              <span className="rounded-sm bg-muted px-1.5 text-nested">
                {member.note}
              </span>
            </dd>
          </>
        )}
      </dl>

      <p className="truncate text-nested text-muted-foreground">
        {copy.team.activityLine(
          member.activity.text,
          formatAge(member.activity.at, openedAt),
        )}
      </p>

      {(member.branch || changed) && (
        <p className="flex items-center gap-2 text-meta tabular-nums">
          {member.branch && (
            <span className="min-w-0 truncate font-mono">{member.branch}</span>
          )}
          {changed && (
            <span className="shrink-0">
              <span className="text-diff-added">+{member.added}</span>{" "}
              <span className="text-diff-removed">−{member.removed}</span>
            </span>
          )}
        </p>
      )}

      <div className="flex items-center gap-1.5">
        <Button
          size="sm"
          onClick={() => useTeamStore.getState().openTerminal(member.id)}
        >
          {copy.team.openTerminal}
        </Button>
        <Button
          size="sm"
          variant="outline"
          onClick={() => useRightPanelStore.getState().show("changes", canDock)}
        >
          {copy.team.changes}
        </Button>
        <DropdownMenu>
          <DropdownMenuTrigger asChild>
            <TooltipIconButton
              label={copy.team.actions(member.role)}
              className="ml-auto"
            >
              <DotsGlyph18 />
            </TooltipIconButton>
          </DropdownMenuTrigger>
          <TeamMemberMenu member={member} />
        </DropdownMenu>
      </div>
    </div>
  );
}
