import { toast } from "sonner";

import { commands, type TerminalInfo } from "@/bindings";
import { errorMessage } from "@/lib/error-message";
import { unwrap } from "@/lib/ipc-result";
import type { TeamMember } from "@/team/team-fixtures";
import { partnersOf, type TeamPair } from "@/team/team-pairs";
import { liveSessionOf } from "@/team/team-sessions";
import { useTerminalStore } from "@/terminal/terminal-store";
import { copy } from "@/ui-copy";

/** The longest message the form sends. A longer one is refused, not cut. */
export const MESSAGE_MAX_LENGTH = 2000;

/**
 * The pause between a message and the Enter that submits it. Written in one go, a long message reads as a paste, and an
 * Enter right behind it is taken as part of that paste, so the message stays in the input box.
 */
export const SUBMIT_DELAY_MS = 150;

/** A member a message can reach, with the session its text is written into. */
export interface MessageRecipient {
  id: string;
  role: string;
  sessionId: string;
}

/** A member can start a conversation when it orchestrates, or has an orchestrator above it. Any other one only answers. */
export function canStartConversation(member: TeamMember): boolean {
  return member.orchestrator || member.parentId !== null;
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

/**
 * The members a member talks with: its pairs, the orchestrator above it and the members under it. A bot can be one of
 * them, though it has no terminal, so a message never reaches it.
 */
export function talksToOf(
  member: TeamMember,
  members: readonly TeamMember[],
  pairs: readonly TeamPair[],
): TeamMember[] {
  const ids = new Set<string | null>([
    ...partnersOf(pairs, member.id),
    member.parentId,
    ...members
      .filter((other) => other.parentId === member.id)
      .map((other) => other.id),
  ]);
  return members.filter((other) => other.id !== member.id && ids.has(other.id));
}

/** The members a message from `member` can reach: those it talks with that have a live session. Bots and sessionless members are not offered. */
export function messageRecipients(
  member: TeamMember,
  members: readonly TeamMember[],
  pairs: readonly TeamPair[],
  sessions: readonly TerminalInfo[],
): MessageRecipient[] {
  return talksToOf(member, members, pairs).flatMap((other) => {
    const session = liveSessionOf(other, sessions);
    return session
      ? [{ id: other.id, role: other.role, sessionId: session.id }]
      : [];
  });
}

/** Whether the text can be sent: not blank, and within the limit. */
export function isSendable(text: string): boolean {
  return text.trim() !== "" && text.length <= MESSAGE_MAX_LENGTH;
}

/**
 * Writes a message into the recipient's terminal, as typing into it would, and then presses Enter. False when it was not
 * sent: a text that is not sendable is refused without a write, and a failed write is shown. The message is not stored
 * in the team; it lives in the recipient's terminal only.
 */
export async function sendTeamMessage(
  senderRole: string,
  sessionId: string,
  text: string,
): Promise<boolean> {
  if (!isSendable(text)) return false;
  try {
    await unwrap(
      commands.writeTerminal(sessionId, `[from ${senderRole}] ${text}`),
    );
    // Input wakes a frozen session, so the terminal store learns it as it does for typing.
    useTerminalStore.getState().woken(sessionId);
    await new Promise((resolve) => setTimeout(resolve, SUBMIT_DELAY_MS));
    // Enter is a carriage return: sent as its own write, so the agent reads it as a key, not as the end of a paste.
    await unwrap(commands.writeTerminal(sessionId, "\r"));
  } catch (error) {
    toast.error(copy.team.message.sendFailed, {
      description: errorMessage(error),
    });
    return false;
  }
  return true;
}
