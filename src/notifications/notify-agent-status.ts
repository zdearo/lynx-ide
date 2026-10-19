import {
  isPermissionGranted,
  requestPermission,
  sendNotification,
} from "@tauri-apps/plugin-notification";

import type { AgentStatusChanged } from "@/bindings";
import { useConfigStore } from "@/config/config-store";
import { agentNotification } from "@/notifications/agent-notification";
import { TERMINAL_PRESETS } from "@/terminal/agent-presets";
import { useTerminalStore } from "@/terminal/terminal-store";
import { useWorkspaceStore } from "@/workspace/workspace-store";
import { worktreeLabel } from "@/workspace/worktree-label";

/** The repository and branch a worktree path belongs to, as one line. */
function placeOf(worktreePath: string): string {
  const { repos, worktrees } = useWorkspaceStore.getState();
  for (const repo of repos) {
    const worktree = worktrees[repo.id]?.find((w) => w.path === worktreePath);
    if (worktree) return `${repo.name} · ${worktreeLabel(worktree).text}`;
  }
  return worktreePath;
}

async function allowed(): Promise<boolean> {
  if (await isPermissionGranted()) return true;
  return (await requestPermission()) === "granted";
}

/**
 * Tells the user, through the system, that an agent finished or needs them. Call it before the
 * terminal store takes the new status, so the previous one is still there to compare.
 */
export async function notifyAgentStatus(
  change: AgentStatusChanged,
): Promise<void> {
  const session = useTerminalStore
    .getState()
    .sessions.find((candidate) => candidate.id === change.id);
  const config = useConfigStore.getState().snapshot?.config.notifications;
  if (!session || !config) return;

  const preset = TERMINAL_PRESETS.find(
    (candidate) => candidate.harness === change.harness,
  );
  const notification = agentNotification({
    previous: session.agentStatus,
    next: change.status,
    config,
    windowActive: document.hasFocus() && document.visibilityState === "visible",
    agentName: preset?.label ?? "Agent",
    place: placeOf(session.worktreePath),
  });
  if (!notification) return;

  try {
    if (await allowed()) sendNotification(notification);
  } catch {
    // A notification is a courtesy: a system that refuses it must not break the status update.
  }
}
