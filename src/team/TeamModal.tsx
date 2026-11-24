import { useEffect, useState, type ReactNode } from "react";
import { toast } from "sonner";

import { BotAvatar } from "@/bots/BotAvatar";
import { botThread } from "@/bots/bot-chat-store";
import { useBotsStore } from "@/bots/bots-store";
import { Conversation } from "@/bots/Conversation";
import { AgentBrandIcon } from "@/components/icons/brands/AgentBrandIcon";
import { XIcon } from "@/components/icons/line-icons";
import { TooltipIconButton } from "@/components/TooltipIconButton";
import { Button } from "@/components/ui/button";
import { errorMessage } from "@/lib/error-message";
import type { TeamAgent, TeamBot, TeamMember } from "@/team/team-fixtures";
import { focusMemberCard } from "@/team/team-focus";
import { MODAL_FRAME_CLASS, MODAL_OVERLAY_CLASS } from "@/team/team-stage";
import { TeamStatusIcon } from "@/team/TeamStatus";
import {
  agentNameOf,
  botOf,
  engineOf,
  isSeededMember,
  useTeamMembers,
  useTeamStore,
} from "@/team/team-store";
import { useTerminalStore } from "@/terminal/terminal-store";
import { useResumableStore } from "@/workspace/resumable-store";
import { copy } from "@/ui-copy";

/**
 * The terminal modal of the team: a title bar over the central area, and a body under it. An agent's terminal is not
 * drawn here: it is on the team's stage, which the body leaves visible. A bot's chat, and an agent with no session,
 * are drawn in the body.
 */
export function TeamModal() {
  const members = useTeamMembers();
  const terminalFor = useTeamStore((state) => state.terminalFor);
  const open = useTeamStore((state) => state.terminalOpen);
  const member = members.find((candidate) => candidate.id === terminalFor);
  // Mounted only while open: the stage keeps its terminals; the modal is only what is drawn over them.
  if (!open || member === undefined) return null;
  return <OpenModal member={member} />;
}

function closeModal() {
  useTeamStore.getState().closeTerminal();
}

function OpenModal({ member }: { member: TeamMember }) {
  // Escape closes it, as it closed the dialog it replaces.
  useEffect(() => {
    const onKeyDown = (event: KeyboardEvent) => {
      if (event.key !== "Escape") return;
      event.preventDefault();
      closeModal();
    };
    window.addEventListener("keydown", onKeyDown, { capture: true });
    return () =>
      window.removeEventListener("keydown", onKeyDown, { capture: true });
  }, []);

  // The modal has no trigger to return focus to, so its member's card gets it once the modal has left. A remount
  // while the modal is still open (StrictMode) does not count as leaving.
  useEffect(
    () => () => {
      if (useTeamStore.getState().terminalOpen) return;
      requestAnimationFrame(() => focusMemberCard(member.id));
    },
    [member.id],
  );

  return (
    <>
      <div className={MODAL_OVERLAY_CLASS} onClick={closeModal} />
      <div role="dialog" aria-modal="true" className={MODAL_FRAME_CLASS}>
        {member.kind === "bot" && <BotChatPane member={member} />}
        {member.kind === "agent" && <TerminalPane member={member} />}
      </div>
    </>
  );
}

/** The title bar of every pane: the member's icon and title, its status, and the close button. */
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
    <header className="pointer-events-auto flex h-9 min-w-0 shrink-0 items-center gap-2 border-b bg-popover px-3 pr-10">
      {icon}
      <h2 className="min-w-0 truncate text-ui font-medium">{title}</h2>
      <span className="ml-auto flex shrink-0 items-center gap-1.5 text-meta text-muted-foreground">
        <TeamStatusIcon status={member.status} />
        {copy.team.status[member.status]}
      </span>
      <TooltipIconButton
        label={
          member.kind === "bot" ? copy.team.closeChat : copy.team.closeTerminal
        }
        className="absolute top-2 right-2"
        onClick={closeModal}
      >
        <XIcon />
      </TooltipIconButton>
    </header>
  );
}

function TerminalPane({ member }: { member: TeamAgent }) {
  const agent = member.harness ? agentNameOf(member.harness) : member.role;
  const session = useTerminalStore((state) =>
    member.sessionId
      ? state.sessions.find((candidate) => candidate.id === member.sessionId)
      : undefined,
  );
  const header = (
    <PaneHeader
      member={member}
      icon={
        <AgentBrandIcon harness={member.harness} className="size-4 shrink-0" />
      }
      title={copy.team.sheetLabel(member.role, agent)}
    />
  );

  if (member.sessionId === null && isSeededMember(member.id)) {
    return (
      <>
        {header}
        <p className="pointer-events-auto border-b bg-background px-3 py-1.5 text-meta text-muted-foreground">
          {copy.team.simulatedTerminal}
        </p>
        <div className="pointer-events-auto min-h-0 flex-1 overflow-y-auto bg-background p-3 font-mono text-ui">
          {member.output.map((line, index) => (
            <p key={index} className="break-words">
              {line}
            </p>
          ))}
        </div>
      </>
    );
  }

  if (!session) {
    return (
      <>
        {header}
        <p className="pointer-events-auto border-b bg-background px-3 py-1.5 text-meta text-muted-foreground">
          {copy.team.terminalClosed}
        </p>
        <ClosedAgentPane key={member.id} member={member} />
      </>
    );
  }

  // The terminal is on the stage, under this body: the body only leaves it visible.
  return (
    <>
      {header}
      <p className="sr-only">{copy.team.terminalDescription}</p>
      <div className="min-h-0 flex-1" />
    </>
  );
}

/**
 * The body of an agent whose session is not open. It offers the agent again: its own conversation when the list of ended
 * sessions still has it, else a new agent in the same worktree. The list is read first, so the label is right.
 */
function ClosedAgentPane({ member }: { member: TeamAgent }) {
  const refresh = useResumableStore((state) => state.refresh);
  const resumable = useResumableStore((state) =>
    state.byWorktree[member.worktree]?.find(
      (item) => item.id === member.sessionId,
    ),
  );
  const [listed, setListed] = useState(false);
  const [starting, setStarting] = useState(false);

  useEffect(() => {
    void refresh(member.worktree).then(() => setListed(true));
  }, [refresh, member.worktree]);

  const revive = () => {
    setStarting(true);
    void useTeamStore
      .getState()
      .reviveMember(member.id)
      .finally(() => setStarting(false));
  };

  return (
    <div className="pointer-events-auto flex min-h-0 flex-1 flex-col items-start gap-2 bg-background p-3">
      {listed && (
        <>
          <Button size="sm" disabled={starting} onClick={revive}>
            {resumable ? copy.team.resume : copy.team.startAgain}
          </Button>
          <p className="text-meta text-muted-foreground">
            {resumable ? copy.team.resumeHint : copy.team.startAgainHint}
          </p>
        </>
      )}
    </div>
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
      <p className="sr-only">{text.log(bot?.name ?? member.role)}</p>
      <div className="pointer-events-auto flex min-h-0 flex-1 flex-col bg-background">
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
