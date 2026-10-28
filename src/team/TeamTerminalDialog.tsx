import { useEffect, type ReactNode } from "react";
import { toast } from "sonner";

import { BotAvatar } from "@/bots/BotAvatar";
import { botThread } from "@/bots/bot-chat-store";
import { useBotsStore } from "@/bots/bots-store";
import { Conversation } from "@/bots/Conversation";
import { AgentBrandIcon } from "@/components/icons/brands/AgentBrandIcon";
import { XmarkGlyph18 } from "@/components/icons/XmarkGlyph18";
import { TooltipIconButton } from "@/components/TooltipIconButton";
import {
  Dialog,
  DialogContent,
  DialogDescription,
  DialogTitle,
} from "@/components/ui/dialog";
import { errorMessage } from "@/lib/error-message";
import type { TeamAgent, TeamBot, TeamMember } from "@/team/team-fixtures";
import { focusMemberCard } from "@/team/team-focus";
import { TeamStatusIcon } from "@/team/TeamStatus";
import {
  agentNameOf,
  botOf,
  engineOf,
  useRunMembers,
  useTeamStore,
} from "@/team/team-store";
import { copy } from "@/ui-copy";

/**
 * A member's terminal, or a bot's chat, in a dialog over the central area. Members have no session in this prototype,
 * so a terminal shows the simulated output from the fixtures and says so.
 */
export function TeamTerminalDialog({
  container,
}: {
  container: HTMLElement | null;
}) {
  const members = useRunMembers();
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
        {member?.kind === "bot" && <BotChatPane member={member} />}
        {member?.kind === "agent" && <TerminalPane member={member} />}
      </DialogContent>
    </Dialog>
  );
}

/** The title bar of both panes: the member's icon and title, its status, and the close button. */
function PaneHeader({
  member,
  icon,
  title,
}: {
  member: TeamMember;
  icon: ReactNode;
  title: string;
}) {
  return (
    <header className="flex min-w-0 items-center gap-2 border-b px-3 py-2 pr-10">
      {icon}
      <DialogTitle className="min-w-0 truncate text-ui font-medium">
        {title}
      </DialogTitle>
      <span className="ml-auto flex shrink-0 items-center gap-1.5 text-meta text-muted-foreground">
        <TeamStatusIcon status={member.status} />
        {copy.team.status[member.status]}
      </span>
      <TooltipIconButton
        label={
          member.kind === "bot" ? copy.team.closeChat : copy.team.closeTerminal
        }
        className="absolute top-2 right-2"
        onClick={() => useTeamStore.getState().closeTerminal()}
      >
        <XmarkGlyph18 />
      </TooltipIconButton>
    </header>
  );
}

function TerminalPane({ member }: { member: TeamAgent }) {
  const agent = agentNameOf(member.harness);

  return (
    <>
      <PaneHeader
        member={member}
        icon={
          <AgentBrandIcon
            harness={member.harness}
            className="size-4 shrink-0"
          />
        }
        title={copy.team.sheetLabel(member.role, agent)}
      />
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

/**
 * A bot's conversation, in the same conversation view the Bots view uses. It shows the bot's most recent thread,
 * with no list of threads beside it. A bot with no thread gets its first one when its threads are read.
 */
function BotChatPane({ member }: { member: TeamBot }) {
  const text = copy.bots.chat;
  const bots = useBotsStore((state) => state.bots);
  const bot = botOf(member, bots);
  const loadThreads = useBotsStore((state) => state.loadThreads);
  const thread = useBotsStore(
    (state) => state.threadsByBot[member.botId]?.[0]?.id,
  );

  useEffect(() => {
    loadThreads(member.botId).catch((error: unknown) =>
      toast.error(copy.bots.threads.loadFailed, {
        description: errorMessage(error),
      }),
    );
  }, [member.botId, loadThreads]);

  return (
    <>
      <PaneHeader
        member={member}
        icon={
          <BotAvatar
            name={member.role}
            color={bot?.color ?? "graphite"}
            size="xs"
          />
        }
        title={copy.team.sheetLabel(member.role, engineOf(member, bots))}
      />
      <DialogDescription className="sr-only">
        {text.log(bot?.name ?? member.role)}
      </DialogDescription>
      <div className="flex min-h-0 flex-1 flex-col bg-background">
        {bot && thread && (
          <Conversation
            key={thread}
            thread={botThread(member.botId, thread)}
            label={text.log(bot.name)}
            placeholder={text.placeholder(bot.name)}
            empty={text.empty(bot.name)}
            loadFailed={text.loadFailed}
            botOf={() => bot}
          />
        )}
      </div>
    </>
  );
}
