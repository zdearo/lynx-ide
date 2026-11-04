import { useEffect, useId, useRef, useState, type FormEvent } from "react";

import { BotAvatar } from "@/bots/BotAvatar";
import { useBotsStore } from "@/bots/bots-store";
import { AgentBrandIcon } from "@/components/icons/brands/AgentBrandIcon";
import { DotsGlyph18 } from "@/components/icons/DotsGlyph18";
import { XmarkGlyph18 } from "@/components/icons/XmarkGlyph18";
import { TooltipIconButton } from "@/components/TooltipIconButton";
import { Button } from "@/components/ui/button";
import {
  DropdownMenu,
  DropdownMenuTrigger,
} from "@/components/ui/dropdown-menu";
import { Input } from "@/components/ui/input";
import { Label } from "@/components/ui/label";
import { DOCK_RIGHT_QUERY } from "@/hooks/layout-queries";
import { useMediaQuery } from "@/hooks/use-media-query";
import { useRightPanelStore } from "@/right-panel/right-panel-store";
import { formatAge } from "@/terminal/relative-time";
import type { TeamMember } from "@/team/team-fixtures";
import { focusMemberCard } from "@/team/team-focus";
import { hasTask } from "@/team/team-hierarchy";
import { partnersOf } from "@/team/team-pairs";
import { TeamMemberMenu } from "@/team/TeamMemberMenu";
import { TeamMessageForm } from "@/team/TeamMessageForm";
import { TeamStatusIcon } from "@/team/TeamStatus";
import {
  botOf,
  engineOf,
  openMember,
  roleOf,
  useTeamStore,
} from "@/team/team-store";
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
  const pairs = useTeamStore((state) => state.pairs);
  const bots = useBotsStore((state) => state.bots);
  const agent = engineOf(member, bots);
  const bot = botOf(member, bots);
  const dependsOn = roleOf(members, member.waitingOn);
  const talksTo = partnersOf(pairs, member.id).flatMap((id) => {
    const role = roleOf(members, id);
    return role ? [{ id, role }] : [];
  });
  const changed = member.added > 0 || member.removed > 0;
  // Ages are read when the panel opens; it is remounted for each member picked.
  const [openedAt] = useState(() => Date.now());
  const fieldId = useId();
  // The task while it is being edited. A refused save keeps it, so the text can be fixed.
  const [draft, setDraft] = useState<{
    task: string;
    acceptance: string;
  } | null>(null);

  async function saveTask(event: FormEvent) {
    event.preventDefault();
    if (!draft) return;
    if (
      await useTeamStore
        .getState()
        .setTask(member.id, draft.task, draft.acceptance)
    ) {
      setDraft(null);
    }
  }

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
        {member.kind === "bot" ? (
          <BotAvatar
            name={member.role}
            color={bot?.color ?? "graphite"}
            size="xs"
          />
        ) : (
          <AgentBrandIcon
            harness={member.harness}
            className="size-4 shrink-0"
          />
        )}
        <h3 className="min-w-0 flex-1 truncate font-medium">{member.role}</h3>
        <span className="min-w-0 max-w-[60%] truncate text-meta text-muted-foreground">
          {agent}
        </span>
      </header>
      <p className="flex items-center gap-1.5 text-meta text-muted-foreground">
        <TeamStatusIcon status={member.status} />
        {copy.team.status[member.status]}
      </p>

      {draft ? (
        <form
          onSubmit={saveTask}
          className="grid gap-3"
          onKeyDown={(event) => {
            // Escape leaves the edit with its text dropped, and does not clear the picked member as it does elsewhere.
            if (event.key !== "Escape") return;
            event.stopPropagation();
            setDraft(null);
          }}
        >
          <div className="grid gap-1.5">
            <Label htmlFor={`${fieldId}-task`}>{copy.team.task}</Label>
            <Input
              id={`${fieldId}-task`}
              value={draft.task}
              autoFocus
              onChange={(event) =>
                setDraft({ ...draft, task: event.target.value })
              }
            />
          </div>
          <div className="grid gap-1.5">
            <Label htmlFor={`${fieldId}-acceptance`}>
              {copy.team.acceptance}
            </Label>
            <Input
              id={`${fieldId}-acceptance`}
              value={draft.acceptance}
              onChange={(event) =>
                setDraft({ ...draft, acceptance: event.target.value })
              }
            />
          </div>
          <div className="flex justify-end gap-1.5">
            <Button
              type="button"
              size="sm"
              variant="outline"
              onClick={() => setDraft(null)}
            >
              {copy.team.add.cancel}
            </Button>
            <Button type="submit" size="sm">
              {copy.team.saveTask}
            </Button>
          </div>
        </form>
      ) : hasTask(member) ? (
        <dl className="grid grid-cols-[auto_minmax(0,1fr)] gap-x-3 gap-y-2">
          <dt className="text-muted-foreground">{copy.team.task}</dt>
          <dd className="min-w-0">{member.task}</dd>
          <dt className="text-muted-foreground">{copy.team.acceptance}</dt>
          <dd className="min-w-0">{member.acceptance}</dd>
        </dl>
      ) : (
        <p className="text-nested text-muted-foreground">{copy.team.noTask}</p>
      )}

      {member.question && (
        <p className="truncate text-nested text-attention">{member.question}</p>
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
        {talksTo.length > 0 && (
          <>
            <dt className="text-muted-foreground">{copy.team.talksTo}</dt>
            <dd className="flex min-w-0 flex-col gap-1">
              {talksTo.map(({ id, role }) => (
                <span key={id} className="flex min-w-0 items-center gap-1">
                  <button
                    type="button"
                    className="min-w-0 flex-1 truncate text-left underline-offset-3 hover:underline"
                    onClick={() => useTeamStore.getState().select(id)}
                  >
                    {role}
                  </button>
                  <TooltipIconButton
                    label={copy.team.disconnect(role)}
                    className="shrink-0"
                    onClick={() =>
                      useTeamStore.getState().setLinked(member.id, id, false)
                    }
                  >
                    <XmarkGlyph18 />
                  </TooltipIconButton>
                </span>
              ))}
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

      {member.activity.text && (
        <p className="truncate text-nested text-muted-foreground">
          {copy.team.activityLine(
            member.activity.text,
            formatAge(member.activity.at, openedAt),
          )}
        </p>
      )}

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

      <TeamMessageForm member={member} members={members} pairs={pairs} />

      <div className="flex items-center gap-1.5">
        <Button size="sm" onClick={() => openMember(member.id)}>
          {member.kind === "bot" ? copy.team.openChat : copy.team.openTerminal}
        </Button>
        {/* A bot has no worktree, so there are no changes to show for it. */}
        {member.kind === "agent" && (
          <Button
            size="sm"
            variant="outline"
            onClick={() =>
              useRightPanelStore.getState().show("changes", canDock)
            }
          >
            {copy.team.changes}
          </Button>
        )}
        {/* Only a member with an orchestrator above it has a task to change. */}
        {hasTask(member) && draft === null && (
          <Button
            size="sm"
            variant="outline"
            onClick={() =>
              setDraft({ task: member.task, acceptance: member.acceptance })
            }
          >
            {copy.team.editTask}
          </Button>
        )}
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
