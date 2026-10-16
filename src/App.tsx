import { useCallback, useEffect, useState } from "react";
import { toast } from "sonner";

import { events } from "@/bindings";
import { Toaster } from "@/components/ui/sonner";
import { Sheet, SheetContent, SheetTitle } from "@/components/ui/sheet";
import { TooltipProvider } from "@/components/ui/tooltip";
import { useMediaQuery } from "@/hooks/use-media-query";
import { errorMessage } from "@/lib/error-message";
import { CustomCommandDialog } from "@/terminal/CustomCommandDialog";
import { useTerminalStore } from "@/terminal/terminal-store";
import { TerminalWorkspace } from "@/terminal/TerminalWorkspace";
import { copy } from "@/ui-copy";
import { useAppShortcuts } from "@/use-app-shortcuts";
import { NewWorktreeDialog } from "@/workspace/NewWorktreeDialog";
import { RemoveRepoDialog } from "@/workspace/RemoveRepoDialog";
import { RemoveWorktreeDialog } from "@/workspace/RemoveWorktreeDialog";
import { Sidebar } from "@/workspace/Sidebar";
import { useWorkspaceStore } from "@/workspace/workspace-store";

/** Below this width the sidebar becomes a drawer so the terminal keeps the room. */
const WIDE_LAYOUT_QUERY = "(min-width: 900px)";
const REFRESH_ON_FOCUS_MS = 2000;

function App() {
  const wide = useMediaQuery(WIDE_LAYOUT_QUERY);
  const [sidebarHidden, setSidebarHidden] = useState(false);
  const [drawerOpen, setDrawerOpen] = useState(false);

  const toggleSidebar = useCallback(() => {
    if (wide) setSidebarHidden((hidden) => !hidden);
    else setDrawerOpen((open) => !open);
  }, [wide]);
  useAppShortcuts(toggleSidebar);

  useEffect(() => {
    Promise.all([
      useWorkspaceStore.getState().load(),
      useTerminalStore.getState().load(),
    ]).catch((error: unknown) => {
      toast.error(copy.sidebar.loadError, { description: errorMessage(error) });
    });

    const unlistenExit = events.terminalExit.listen(({ payload }) => {
      useTerminalStore.getState().markExited(payload.id, payload.exitCode);
    });

    // Worktrees can change outside the app; coming back to the window is when it matters.
    let lastRefresh = 0;
    const refreshOnFocus = () => {
      if (Date.now() - lastRefresh < REFRESH_ON_FOCUS_MS) return;
      lastRefresh = Date.now();
      void useWorkspaceStore.getState().refreshAll();
    };
    window.addEventListener("focus", refreshOnFocus);

    return () => {
      window.removeEventListener("focus", refreshOnFocus);
      void unlistenExit.then((unlisten) => unlisten());
    };
  }, []);

  const sidebarVisible = wide && !sidebarHidden;

  return (
    <TooltipProvider delayDuration={400}>
      <div className="flex h-full bg-background text-foreground">
        {sidebarVisible && (
          <aside className="w-64 shrink-0 border-r border-sidebar-border">
            <Sidebar />
          </aside>
        )}
        <TerminalWorkspace
          sidebarVisible={sidebarVisible}
          onToggleSidebar={toggleSidebar}
        />
      </div>

      {!wide && (
        <Sheet open={drawerOpen} onOpenChange={setDrawerOpen}>
          <SheetContent
            side="left"
            onOpenAutoFocus={(event) => {
              // Land on the current worktree, not on the first button (which would pop its tooltip).
              event.preventDefault();
              document
                .querySelector<HTMLElement>(
                  '[data-slot="sheet-content"] [aria-current="true"]',
                )
                ?.focus();
            }}
            className="gap-0 p-0 data-[side=left]:w-72 data-[side=left]:sm:max-w-72"
            showCloseButton={false}
          >
            <SheetTitle className="sr-only">{copy.sidebar.title}</SheetTitle>
            <Sidebar onNavigate={() => setDrawerOpen(false)} />
          </SheetContent>
        </Sheet>
      )}

      <NewWorktreeDialog />
      <RemoveWorktreeDialog />
      <RemoveRepoDialog />
      <CustomCommandDialog />
      <Toaster position="bottom-right" />
    </TooltipProvider>
  );
}

export default App;
