import type { TeamLastMessage, TerminalInfo } from "@/bindings";
import type {
  TeamActionKind,
  TeamActivity,
  TeamMember,
} from "@/team/team-fixtures";
import { liveSessionOf } from "@/team/team-sessions";
import { formatAge } from "@/terminal/relative-time";
import { copy } from "@/ui-copy";

const ACTION_KINDS: readonly TeamActionKind[] = [
  "done",
  "failed",
  "noReport",
  "needsYou",
  "question",
  "answer",
];

function isActionKind(kind: string): kind is TeamActionKind {
  return (ACTION_KINDS as readonly string[]).includes(kind);
}

/** The kind a message's stored text names, or "other" for a kind this screen does not know: it never throws on one. */
export function actionKindOf(kind: string): TeamActionKind {
  return isActionKind(kind) ? kind : "other";
}

/** A member's newest message as its activity: the text and its kind. Null when it has no message. */
export function activityOf(
  message: TeamLastMessage | null,
): TeamActivity | null {
  if (!message) return null;
  return {
    text: message.body,
    at: message.createdAt,
    kind: actionKindOf(message.kind),
  };
}

/**
 * How long a member has been running, for its card and its sheet: "Running 5m", or "Just started" in the first minute.
 * Null when it runs nothing: a bot, an agent with no live session, or a member that is done, failed or closed, even if
 * its session still runs.
 */
export function runningLabelOf(
  member: TeamMember,
  sessions: readonly TerminalInfo[],
  nowMs: number,
): string | null {
  if (
    member.status === "done" ||
    member.status === "failed" ||
    member.status === "closed"
  ) {
    return null;
  }
  const session = liveSessionOf(member, sessions);
  if (session === undefined) return null;
  const age = formatAge(session.startedAt, nowMs);
  return age === "now" ? copy.team.justStarted : copy.team.runningFor(age);
}
