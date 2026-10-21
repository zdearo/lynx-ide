import {
  isPermissionGranted,
  requestPermission,
  sendNotification,
} from "@tauri-apps/plugin-notification";

export interface SystemNotice {
  title: string;
  body: string;
}

async function allowed(): Promise<boolean> {
  if (await isPermissionGranted()) return true;
  return (await requestPermission()) === "granted";
}

/** Shows a notice through the system. A system that refuses it must not break the caller. */
export async function sendSystemNotice(notice: SystemNotice): Promise<void> {
  try {
    if (await allowed()) sendNotification(notice);
  } catch {
    // A notification is a courtesy: a system that refuses it must not break the caller.
  }
}
