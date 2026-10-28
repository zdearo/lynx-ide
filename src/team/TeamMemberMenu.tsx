import { useBotsStore } from "@/bots/bots-store";
import {
  ContextMenuCheckboxItem,
  ContextMenuContent,
  ContextMenuItem,
  ContextMenuSeparator,
  ContextMenuSub,
  ContextMenuSubContent,
  ContextMenuSubTrigger,
} from "@/components/ui/context-menu";
import {
  DropdownMenuCheckboxItem,
  DropdownMenuContent,
  DropdownMenuItem,
  DropdownMenuSeparator,
  DropdownMenuSub,
  DropdownMenuSubContent,
  DropdownMenuSubTrigger,
} from "@/components/ui/dropdown-menu";
import { TEAM_ROLES, type TeamMember } from "@/team/team-fixtures";
import { orchestratorOptionsOf } from "@/team/team-hierarchy";
import { linkOptionsOf } from "@/team/team-pairs";
import {
  engineOf,
  openMember,
  useRunMembers,
  useTeamStore,
} from "@/team/team-store";
import { copy } from "@/ui-copy";

/** The actions of one member on a tree row or in the sheet. Give it to a DropdownMenu. */
export function TeamMemberMenu({ member }: { member: TeamMember }) {
  const members = useRunMembers();
  const pairs = useTeamStore((state) => state.pairs);
  const bots = useBotsStore((state) => state.bots);
  const hasMembers = members.some((other) => other.parentId === member.id);
  // A bot has no terminal and no role to take, and it is always an orchestrator under no one.
  const isBot = member.kind === "bot";

  return (
    <DropdownMenuContent align="end" className="min-w-52">
      <DropdownMenuItem onSelect={() => openMember(member.id)}>
        {isBot ? copy.team.openChat : copy.team.openTerminal}
      </DropdownMenuItem>
      {!isBot && (
        <DropdownMenuSub>
          <DropdownMenuSubTrigger>{copy.team.reassign}</DropdownMenuSubTrigger>
          <DropdownMenuSubContent>
            {TEAM_ROLES.filter((role) => role !== member.role).map((role) => (
              <DropdownMenuItem
                key={role}
                onSelect={() =>
                  useTeamStore.getState().setRole(member.id, role)
                }
              >
                {role}
              </DropdownMenuItem>
            ))}
          </DropdownMenuSubContent>
        </DropdownMenuSub>
      )}
      <DropdownMenuCheckboxItem
        checked={member.orchestrator}
        disabled={isBot || hasMembers}
        onCheckedChange={(next) =>
          useTeamStore.getState().setOrchestrator(member.id, next)
        }
      >
        {copy.team.orchestrator}
      </DropdownMenuCheckboxItem>
      <DropdownMenuSub>
        <DropdownMenuSubTrigger disabled={isBot}>
          {copy.team.answersTo}
        </DropdownMenuSubTrigger>
        <DropdownMenuSubContent>
          <DropdownMenuCheckboxItem
            checked={member.parentId === null}
            onCheckedChange={() =>
              useTeamStore.getState().setParent(member.id, null)
            }
          >
            {copy.team.answersToNoOne}
          </DropdownMenuCheckboxItem>
          {orchestratorOptionsOf(member.id, members).map(
            ({ member: other, current, enabled }) => (
              <DropdownMenuCheckboxItem
                key={other.id}
                checked={current}
                disabled={!enabled}
                onCheckedChange={() =>
                  useTeamStore.getState().setParent(member.id, other.id)
                }
              >
                {copy.team.sheetLabel(other.role, engineOf(other, bots))}
              </DropdownMenuCheckboxItem>
            ),
          )}
        </DropdownMenuSubContent>
      </DropdownMenuSub>
      <DropdownMenuSub>
        <DropdownMenuSubTrigger>{copy.team.connect}</DropdownMenuSubTrigger>
        <DropdownMenuSubContent>
          {linkOptionsOf(member.id, members, pairs).map(
            ({ member: other, linked, enabled }) => (
              <DropdownMenuCheckboxItem
                key={other.id}
                checked={linked}
                disabled={!enabled}
                onCheckedChange={(next) =>
                  useTeamStore.getState().setLinked(member.id, other.id, next)
                }
              >
                {copy.team.sheetLabel(other.role, engineOf(other, bots))}
              </DropdownMenuCheckboxItem>
            ),
          )}
        </DropdownMenuSubContent>
      </DropdownMenuSub>
      <DropdownMenuSeparator />
      <DropdownMenuItem
        onSelect={() => useTeamStore.getState().dismiss(member.id)}
      >
        {copy.team.dismiss}
      </DropdownMenuItem>
    </DropdownMenuContent>
  );
}

/** The same actions on a map card's right-click menu. Give it to a ContextMenu. */
export function TeamMemberContextMenu({ member }: { member: TeamMember }) {
  const members = useRunMembers();
  const pairs = useTeamStore((state) => state.pairs);
  const bots = useBotsStore((state) => state.bots);
  const hasMembers = members.some((other) => other.parentId === member.id);
  const isBot = member.kind === "bot";

  return (
    <ContextMenuContent className="min-w-52">
      <ContextMenuItem onSelect={() => openMember(member.id)}>
        {isBot ? copy.team.openChat : copy.team.openTerminal}
      </ContextMenuItem>
      {!isBot && (
        <ContextMenuSub>
          <ContextMenuSubTrigger>{copy.team.reassign}</ContextMenuSubTrigger>
          <ContextMenuSubContent>
            {TEAM_ROLES.filter((role) => role !== member.role).map((role) => (
              <ContextMenuItem
                key={role}
                onSelect={() =>
                  useTeamStore.getState().setRole(member.id, role)
                }
              >
                {role}
              </ContextMenuItem>
            ))}
          </ContextMenuSubContent>
        </ContextMenuSub>
      )}
      <ContextMenuCheckboxItem
        checked={member.orchestrator}
        disabled={isBot || hasMembers}
        onCheckedChange={(next) =>
          useTeamStore.getState().setOrchestrator(member.id, next)
        }
      >
        {copy.team.orchestrator}
      </ContextMenuCheckboxItem>
      <ContextMenuSub>
        <ContextMenuSubTrigger disabled={isBot}>
          {copy.team.answersTo}
        </ContextMenuSubTrigger>
        <ContextMenuSubContent>
          <ContextMenuCheckboxItem
            checked={member.parentId === null}
            onCheckedChange={() =>
              useTeamStore.getState().setParent(member.id, null)
            }
          >
            {copy.team.answersToNoOne}
          </ContextMenuCheckboxItem>
          {orchestratorOptionsOf(member.id, members).map(
            ({ member: other, current, enabled }) => (
              <ContextMenuCheckboxItem
                key={other.id}
                checked={current}
                disabled={!enabled}
                onCheckedChange={() =>
                  useTeamStore.getState().setParent(member.id, other.id)
                }
              >
                {copy.team.sheetLabel(other.role, engineOf(other, bots))}
              </ContextMenuCheckboxItem>
            ),
          )}
        </ContextMenuSubContent>
      </ContextMenuSub>
      <ContextMenuSub>
        <ContextMenuSubTrigger>{copy.team.connect}</ContextMenuSubTrigger>
        <ContextMenuSubContent>
          {linkOptionsOf(member.id, members, pairs).map(
            ({ member: other, linked, enabled }) => (
              <ContextMenuCheckboxItem
                key={other.id}
                checked={linked}
                disabled={!enabled}
                onCheckedChange={(next) =>
                  useTeamStore.getState().setLinked(member.id, other.id, next)
                }
              >
                {copy.team.sheetLabel(other.role, engineOf(other, bots))}
              </ContextMenuCheckboxItem>
            ),
          )}
        </ContextMenuSubContent>
      </ContextMenuSub>
      <ContextMenuSeparator />
      <ContextMenuItem
        onSelect={() => useTeamStore.getState().dismiss(member.id)}
      >
        {copy.team.dismiss}
      </ContextMenuItem>
    </ContextMenuContent>
  );
}
