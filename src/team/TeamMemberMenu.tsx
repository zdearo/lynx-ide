import { toast } from "sonner";

import {
  ContextMenuContent,
  ContextMenuItem,
  ContextMenuSeparator,
  ContextMenuSub,
  ContextMenuSubContent,
  ContextMenuSubTrigger,
} from "@/components/ui/context-menu";
import {
  DropdownMenuContent,
  DropdownMenuItem,
  DropdownMenuSeparator,
  DropdownMenuSub,
  DropdownMenuSubContent,
  DropdownMenuSubTrigger,
} from "@/components/ui/dropdown-menu";
import { TEAM_ROLES, type TeamMember } from "@/team/team-fixtures";
import { useTeamStore } from "@/team/team-store";
import { copy } from "@/ui-copy";

/** The actions of one member on a tree row or in the sheet. Give it to a DropdownMenu. */
export function TeamMemberMenu({ member }: { member: TeamMember }) {
  const isLead = member.parentId === null;

  return (
    <DropdownMenuContent align="end" className="min-w-52">
      <DropdownMenuItem
        onSelect={() => useTeamStore.getState().openTerminal(member.id)}
      >
        {copy.team.openTerminal}
      </DropdownMenuItem>
      <DropdownMenuSub>
        <DropdownMenuSubTrigger disabled={isLead}>
          {copy.team.reassign}
        </DropdownMenuSubTrigger>
        <DropdownMenuSubContent>
          {TEAM_ROLES.filter((role) => role !== member.role).map((role) => (
            <DropdownMenuItem
              key={role}
              onSelect={() => useTeamStore.getState().setRole(member.id, role)}
            >
              {role}
            </DropdownMenuItem>
          ))}
        </DropdownMenuSubContent>
      </DropdownMenuSub>
      <DropdownMenuItem onSelect={() => toast.info(copy.team.connectNotWired)}>
        {copy.team.connect}
      </DropdownMenuItem>
      <DropdownMenuSeparator />
      <DropdownMenuItem
        disabled={isLead}
        onSelect={() => useTeamStore.getState().dismiss(member.id)}
      >
        {copy.team.dismiss}
      </DropdownMenuItem>
    </DropdownMenuContent>
  );
}

/** The same actions on a map card's right-click menu. Give it to a ContextMenu. */
export function TeamMemberContextMenu({ member }: { member: TeamMember }) {
  const isLead = member.parentId === null;

  return (
    <ContextMenuContent className="min-w-52">
      <ContextMenuItem
        onSelect={() => useTeamStore.getState().openTerminal(member.id)}
      >
        {copy.team.openTerminal}
      </ContextMenuItem>
      <ContextMenuSub>
        <ContextMenuSubTrigger disabled={isLead}>
          {copy.team.reassign}
        </ContextMenuSubTrigger>
        <ContextMenuSubContent>
          {TEAM_ROLES.filter((role) => role !== member.role).map((role) => (
            <ContextMenuItem
              key={role}
              onSelect={() => useTeamStore.getState().setRole(member.id, role)}
            >
              {role}
            </ContextMenuItem>
          ))}
        </ContextMenuSubContent>
      </ContextMenuSub>
      <ContextMenuItem onSelect={() => toast.info(copy.team.connectNotWired)}>
        {copy.team.connect}
      </ContextMenuItem>
      <ContextMenuSeparator />
      <ContextMenuItem
        disabled={isLead}
        onSelect={() => useTeamStore.getState().dismiss(member.id)}
      >
        {copy.team.dismiss}
      </ContextMenuItem>
    </ContextMenuContent>
  );
}
