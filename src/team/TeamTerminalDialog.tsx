import { AgentBrandIcon } from "@/components/icons/brands/AgentBrandIcon";
import { XmarkGlyph18 } from "@/components/icons/XmarkGlyph18";
import { TooltipIconButton } from "@/components/TooltipIconButton";
import {
  Dialog,
  DialogContent,
  DialogDescription,
  DialogTitle,
} from "@/components/ui/dialog";
import type { TeamMember } from "@/team/team-fixtures";
import { focusMemberCard } from "@/team/team-focus";
import { TeamStatusIcon } from "@/team/TeamTree";
import { agentNameOf, useTeamStore } from "@/team/team-store";
import { copy } from "@/ui-copy";

/**
 * A member's terminal, in a dialog over the central area. Members have no session in this prototype,
 * so the body shows the simulated output from the fixtures and says so.
 */
export function TeamTerminalDialog({
  container,
}: {
  container: HTMLElement | null;
}) {
  const members = useTeamStore((state) => state.members);
  const terminalFor = useTeamStore((state) => state.terminalFor);
  const open = useTeamStore((state) => state.terminalOpen);
  const member = members.find((candidate) => candidate.id === terminalFor);

  return (
    <Dialog
      open={open && member !== undefined}
      onOpenChange={(next) => {
        if (!next) useTeamStore.getState().closeTerminal();
      }}
    >
      <DialogContent
        container={container}
        showCloseButton={false}
        // The first button would open its tooltip, and then Escape would close only the tooltip.
        onOpenAutoFocus={(event) => {
          event.preventDefault();
          (event.currentTarget as HTMLElement).focus();
        }}
        // The dialog has no trigger to return focus to, so the member's card gets it.
        onCloseAutoFocus={(event) => {
          event.preventDefault();
          focusMemberCard(terminalFor);
        }}
        className="absolute inset-[5%] flex h-auto w-auto max-w-none translate-none flex-col gap-0 overflow-hidden p-0 sm:max-w-none"
      >
        {member && <TerminalPane member={member} />}
      </DialogContent>
    </Dialog>
  );
}

function TerminalPane({ member }: { member: TeamMember }) {
  const agent = agentNameOf(member.harness);

  return (
    <>
      <header className="flex min-w-0 items-center gap-2 border-b px-3 py-2 pr-10">
        <AgentBrandIcon harness={member.harness} className="size-4 shrink-0" />
        <DialogTitle className="min-w-0 truncate text-ui font-medium">
          {copy.team.sheetLabel(member.role, agent)}
        </DialogTitle>
        <span className="ml-auto flex shrink-0 items-center gap-1.5 text-meta text-muted-foreground">
          <TeamStatusIcon status={member.status} />
          {copy.team.status[member.status]}
        </span>
        <TooltipIconButton
          label={copy.team.closeTerminal}
          className="absolute top-2 right-2"
          onClick={() => useTeamStore.getState().closeTerminal()}
        >
          <XmarkGlyph18 />
        </TooltipIconButton>
      </header>
      <DialogDescription className="border-b px-3 py-1.5 text-meta">
        {copy.team.simulatedTerminal}
      </DialogDescription>
      <div className="min-h-0 flex-1 overflow-y-auto bg-background p-3 font-mono text-ui">
        {member.output.map((line, index) => (
          <p key={index} className="break-words">
            {line}
          </p>
        ))}
      </div>
    </>
  );
}
