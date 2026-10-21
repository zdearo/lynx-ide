import type { NotificationsConfig, UpdateStatus } from "@/bindings";
import type { SystemNotice } from "@/notifications/system-notification";
import { copy } from "@/ui-copy";

export interface UpdateAnnouncement {
  version: string;
  /** The system notice too, when that kind is on and the window is not in front. */
  system: SystemNotice | null;
}

/**
 * What to tell the user about a new version, or null when there is nothing new. A version is
 * announced once: a periodic check that finds the same release again stays quiet.
 */
export function updateAnnouncement({
  status,
  announced,
  config,
  windowActive,
}: {
  status: UpdateStatus;
  announced: string | null;
  config: NotificationsConfig;
  windowActive: boolean;
}): UpdateAnnouncement | null {
  const { phase } = status;
  if (phase.kind !== "available" || phase.version === announced) return null;

  const system =
    config.onUpdate && !windowActive
      ? {
          title: copy.notifications.updateAvailable(phase.version),
          body: copy.notifications.updateBody,
        }
      : null;
  return { version: phase.version, system };
}
