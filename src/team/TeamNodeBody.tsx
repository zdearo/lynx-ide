import { useEffect, useState } from "react";
import { toast } from "sonner";

import { botThread } from "@/bots/bot-chat-store";
import { useBotsStore } from "@/bots/bots-store";
import { Conversation } from "@/bots/Conversation";
import { Button } from "@/components/ui/button";
import { errorMessage } from "@/lib/error-message";
import type { TeamAgent, TeamBot, TeamMember } from "@/team/team-fixtures";
import { botOf, isSeededMember, useTeamStore } from "@/team/team-store";
import { TerminalView } from "@/terminal/TerminalView";
import { useTerminalStore } from "@/terminal/terminal-store";
import { copy } from "@/ui-copy";

/**
 * What an expanded card shows under its title: an agent's terminal, a bot's chat, or what a member without a session
 * shows in its place. The title and the close button belong to the card.
 */
export function TeamNodeBody({
  member,
  onScreen,
  resizeLive,
}: {
  member: TeamMember;
  onScreen: boolean;
  resizeLive: boolean;
}) {
  if (member.kind === "bot") return <BotChatBody member={member} />;
  return (
    <AgentBody member={member} onScreen={onScreen} resizeLive={resizeLive} />
  );
}

function AgentBody({
  member,
  onScreen,
  resizeLive,
}: {
  member: TeamAgent;
  onScreen: boolean;
  resizeLive: boolean;
}) {
  const moving = useTeamStore((state) => state.movingIds.includes(member.id));
  // Of the open terminals, the picked one takes the keyboard.
  const focused = useTeamStore((state) => state.selectedId === member.id);
  const session = useTerminalStore((state) =>
    member.sessionId
      ? state.sessions.find((candidate) => candidate.id === member.sessionId)
      : undefined,
  );

  if (member.sessionId === null && isSeededMember(member.id)) {
    return (
      <div className="flex min-h-0 flex-1 flex-col bg-background">
        <p className="border-b px-3 py-1.5 text-meta text-muted-foreground">
          {copy.team.simulatedTerminal}
        </p>
        <div className="min-h-0 flex-1 overflow-y-auto p-3 font-mono text-ui">
          {member.output.map((line, index) => (
            <p key={index} className="break-words">
              {line}
            </p>
          ))}
        </div>
      </div>
    );
  }

  if (!session) {
    return <ClosedAgentBody member={member} />;
  }

  // An open terminal stays mounted, on or off the screen: the screen only says which ones are in view.
  return (
    <div className="flex min-h-0 flex-1 flex-col bg-background">
      <TerminalView
        terminalId={session.id}
        active
        focused={focused}
        inView={onScreen}
        resizeHeld={moving}
        resizeLive={resizeLive}
      />
    </div>
  );
}

/**
 * The body of an agent whose session is gone for good: a session that was open when the app quit is restored from its
 * own screen instead. It starts a new agent in the same worktree.
 */
function ClosedAgentBody({ member }: { member: TeamAgent }) {
  const [starting, setStarting] = useState(false);

  const revive = () => {
    setStarting(true);
    void useTeamStore
      .getState()
      .reviveMember(member.id)
      .finally(() => setStarting(false));
  };

  return (
    <div className="flex min-h-0 flex-1 flex-col items-start gap-2 bg-background p-3">
      <p className="text-meta text-muted-foreground">
        {copy.team.terminalClosed}
      </p>
      <Button size="sm" disabled={starting} onClick={revive}>
        {copy.team.startAgain}
      </Button>
      <p className="text-meta text-muted-foreground">
        {copy.team.startAgainHint}
      </p>
    </div>
  );
}

/**
 * A bot's conversation, in the same conversation view the Bots view uses. It shows the bot's most recent thread,
 * with no list of threads beside it. A bot with no thread gets its first one when its threads are read.
 */
function BotChatBody({ member }: { member: TeamBot }) {
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
    <div className="flex min-h-0 flex-1 flex-col bg-background">
      <p className="sr-only">{text.log(bot?.name ?? member.role)}</p>
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
  );
}
