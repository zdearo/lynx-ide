import { useEffect } from "react";
import { getCurrentWindow } from "@tauri-apps/api/window";

import { commands } from "@/bindings";
import { useDialogStore } from "@/dialog-store";

/** Closes the window for real, so the close request does not come back to the guard. */
export async function closeWindow(): Promise<void> {
  try {
    await getCurrentWindow().destroy();
  } catch {
    // Outside the app there is no window to close.
  }
}

/**
 * Closing the window while the daemon holds terminals asks first: they can stay running for the
 * next start, or end now. Without any, the window closes as it always did.
 */
export function useQuitGuard(): void {
  useEffect(() => {
    let unlisten: (() => void) | undefined;
    try {
      getCurrentWindow()
        .onCloseRequested(async (event) => {
          const status = await commands.ptyDaemonStatus();
          if (status.status !== "ok" || status.data.sessions === 0) return;
          event.preventDefault();
          useDialogStore.getState().askQuit(status.data.sessions);
        })
        .then((stop) => {
          unlisten = stop;
        })
        .catch(() => {
          // Outside the app there is no window to watch.
        });
    } catch {
      // Outside the app there is no window to watch.
    }
    return () => unlisten?.();
  }, []);
}
