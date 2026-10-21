import { toast } from "sonner";

import type { UpdateStatus } from "@/bindings";
import { useConfigStore } from "@/config/config-store";
import { useNavigationStore } from "@/navigation/navigation-store";
import { sendSystemNotice } from "@/notifications/system-notification";
import { updateAnnouncement } from "@/updates/update-notification";
import { copy } from "@/ui-copy";

/** The version announced in this session, so the same release is not announced twice. */
let announced: string | null = null;

/**
 * Tells the user a new version is out: a notice in the app that stays until it is dismissed, and
 * a system notification when that kind is on and the window is not in front.
 */
export function notifyUpdate(status: UpdateStatus): void {
  const config = useConfigStore.getState().snapshot?.config.notifications;
  if (!config) return;

  const announcement = updateAnnouncement({
    status,
    announced,
    config,
    windowActive: document.hasFocus() && document.visibilityState === "visible",
  });
  if (!announcement) return;

  announced = announcement.version;
  toast(copy.notifications.updateAvailable(announcement.version), {
    description: copy.notifications.updateBody,
    duration: Number.POSITIVE_INFINITY,
    action: {
      label: copy.notifications.updateView,
      onClick: () => useNavigationStore.getState().openSettings("updates"),
    },
  });
  if (announcement.system) void sendSystemNotice(announcement.system);
}
