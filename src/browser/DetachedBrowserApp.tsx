import { useEffect } from "react";

import { commands, events } from "@/bindings";
import { BrowserView } from "@/browser/BrowserView";
import { useBrowserStore } from "@/browser/browser-store";
import { TooltipProvider } from "@/components/ui/tooltip";
import { Toaster } from "@/components/ui/sonner";

/** What the browser window shows: the browser, and nothing else of the app. */
export function DetachedBrowserApp() {
  useEffect(() => {
    const unlisten = events.browserStateChanged.listen(({ payload }) => {
      useBrowserStore.getState().apply(payload.state);
    });
    const unlistenAgent = events.browserAgentActivity.listen(({ payload }) => {
      useBrowserStore
        .getState()
        .noteAgentAction(payload.page, payload.agent, payload.summary);
    });
    const unlistenReleased = events.browserAgentReleased.listen(() => {
      useBrowserStore.getState().releaseAgent();
    });
    // The state may have changed before this window was listening.
    void commands.browserState().then((result) => {
      if (result.status === "ok") useBrowserStore.getState().apply(result.data);
    });
    return () => {
      void unlisten.then((stop) => stop());
      void unlistenAgent.then((stop) => stop());
      void unlistenReleased.then((stop) => stop());
    };
  }, []);

  return (
    <TooltipProvider delayDuration={400}>
      <div className="relative h-full bg-background text-foreground">
        <BrowserView />
      </div>
      <Toaster />
    </TooltipProvider>
  );
}
