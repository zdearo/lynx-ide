import type { AgentStatus, NotificationsConfig } from "@/bindings";
import { copy } from "@/ui-copy";

export interface AgentNotification {
  title: string;
  body: string;
}

interface AgentNotificationInput {
  previous: AgentStatus | null;
  next: AgentStatus | null;
  config: NotificationsConfig;
  /** The window is in front and visible: the sidebar already shows what changed. */
  windowActive: boolean;
  agentName: string;
  /** Where the agent runs: the repository and the worktree's branch. */
  place: string;
}

/**
 * What to tell the user when an agent's status changes, or null when nothing is worth a system
 * notification: only a change into "done" or "needs you", only if that kind is on, and only while
 * the window is not in front.
 */
export function agentNotification({
  previous,
  next,
  config,
  windowActive,
  agentName,
  place,
}: AgentNotificationInput): AgentNotification | null {
  if (windowActive || previous === next) return null;
  if (next === "done" && config.onDone) {
    return { title: copy.notifications.done(agentName), body: place };
  }
  if (next === "needsYou" && config.onNeedsYou) {
    return { title: copy.notifications.needsYou(agentName), body: place };
  }
  return null;
}
