import {
  ContextMenuCheckboxItem,
  ContextMenuContent,
  ContextMenuItem,
  ContextMenuSeparator,
} from "@/components/ui/context-menu";
import {
  DropdownMenuCheckboxItem,
  DropdownMenuContent,
  DropdownMenuItem,
  DropdownMenuSeparator,
} from "@/components/ui/dropdown-menu";
import type { TeamMember } from "@/team/team-fixtures";
import { openMember, useTeamMembers, useTeamStore } from "@/team/team-store";
import { copy } from "@/ui-copy";

// What a member's menu offers. The lines (who answers to whom, who talks with whom) are drawn on the map, not picked here.

/** The actions of one member in the sheet. Give it to a DropdownMenu. */
export function TeamMemberMenu({ member }: { member: TeamMember }) {
  const members = useTeamMembers();
  const hasMembers = members.some((other) => other.parentId === member.id);
  // A bot has no terminal, and it is always an orchestrator under no one.
  const isBot = member.kind === "bot";

  return (
    <DropdownMenuContent align="end" className="min-w-52">
      <DropdownMenuItem onSelect={() => openMember(member.id)}>
        {isBot ? copy.team.openChat : copy.team.openTerminal}
      </DropdownMenuItem>
      <DropdownMenuCheckboxItem
        checked={member.orchestrator}
        disabled={isBot || hasMembers}
        onCheckedChange={(next) =>
          useTeamStore.getState().setOrchestrator(member.id, next)
        }
      >
        {copy.team.orchestrator}
      </DropdownMenuCheckboxItem>
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
  const members = useTeamMembers();
  const hasMembers = members.some((other) => other.parentId === member.id);
  const isBot = member.kind === "bot";

  return (
    <ContextMenuContent className="min-w-52">
      <ContextMenuItem onSelect={() => openMember(member.id)}>
        {isBot ? copy.team.openChat : copy.team.openTerminal}
      </ContextMenuItem>
      <ContextMenuCheckboxItem
        checked={member.orchestrator}
        disabled={isBot || hasMembers}
        onCheckedChange={(next) =>
          useTeamStore.getState().setOrchestrator(member.id, next)
        }
      >
        {copy.team.orchestrator}
      </ContextMenuCheckboxItem>
      <ContextMenuSeparator />
      <ContextMenuItem
        onSelect={() => useTeamStore.getState().dismiss(member.id)}
      >
        {copy.team.dismiss}
      </ContextMenuItem>
    </ContextMenuContent>
  );
}
