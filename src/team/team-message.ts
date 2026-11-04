import { toast } from "sonner";

import { commands, type TerminalInfo } from "@/bindings";
import { errorMessage } from "@/lib/error-message";
import { unwrap } from "@/lib/ipc-result";
import type { TeamMember } from "@/team/team-fixtures";
import { partnersOf, type TeamPair } from "@/team/team-pairs";
import { useTerminalStore } from "@/terminal/terminal-store";
import { copy } from "@/ui-copy";

/** The longest message the form sends. A longer one is refused, not cut. */
export const MESSAGE_MAX_LENGTH = 2000;

/** A paired member a message can reach, with the session its text is written into. */
export interface MessageRecipient {
  id: string;
  role: string;
  sessionId: string;
}

/** A member can start a conversation when it orchestrates, or has an orchestrator above it. Any other one only answers. */
export function canStartConversation(member: TeamMember): boolean {
  return member.orchestrator || member.parentId !== null;
}

/** The session a member runs in, when it is an agent whose session is still running. */
function liveSessionOf(
  member: TeamMember,
  sessions: readonly TerminalInfo[],
): TerminalInfo | undefined {
  if (member.kind !== "agent" || member.sessionId === null) return undefined;
  return sessions.find(
    (session) => session.id === member.sessionId && session.running,
  );
}

/** Whether the screen offers the message form for a member: it can start conversations and has a live session. */
export function showsMessageForm(
  member: TeamMember,
  sessions: readonly TerminalInfo[],
): boolean {
  return (
    canStartConversation(member) &&
    liveSessionOf(member, sessions) !== undefined
  );
}

/** The paired members a message from `member` can reach: those with a live session. Bots and sessionless members are not offered. */
export function messageRecipients(
  member: TeamMember,
  members: readonly TeamMember[],
  pairs: readonly TeamPair[],
  sessions: readonly TerminalInfo[],
): MessageRecipient[] {
  const partners = partnersOf(pairs, member.id);
  return members.flatMap((other) => {
    const session = partners.includes(other.id)
      ? liveSessionOf(other, sessions)
      : undefined;
    return session
      ? [{ id: other.id, role: other.role, sessionId: session.id }]
      : [];
  });
}

/** Whether the text can be sent: not blank, and within the limit. */
export function isSendable(text: string): boolean {
  return text.trim() !== "" && text.length <= MESSAGE_MAX_LENGTH;
}

/** The line written into the recipient's terminal: the sender's role, the text as typed, and Enter. */
export function messageLine(senderRole: string, text: string): string {
  // Enter is a carriage return in a terminal: the same as the orchestration sends a reply.
  return `[from ${senderRole}] ${text}\r`;
}

/**
 * Writes a message into the recipient's terminal, as typing into it would. False when it was not sent: a text that is
 * not sendable is refused without a write, and a failed write is shown. The message is not stored in the team; it
 * lives in the recipient's terminal only.
 */
export async function sendTeamMessage(
  senderRole: string,
  sessionId: string,
  text: string,
): Promise<boolean> {
  if (!isSendable(text)) return false;
  try {
    await unwrap(
      commands.writeTerminal(sessionId, messageLine(senderRole, text)),
    );
  } catch (error) {
    toast.error(copy.team.message.sendFailed, {
      description: errorMessage(error),
    });
    return false;
  }
  // Input wakes a frozen session, so the terminal store learns it as it does for typing.
  useTerminalStore.getState().woken(sessionId);
  return true;
}
