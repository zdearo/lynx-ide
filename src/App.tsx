import { useCallback, useEffect } from "react";
import { toast } from "sonner";

import { events } from "@/bindings";
import { useConfigStore } from "@/config/config-store";
import { useExplorerStore } from "@/files/explorer-store";
import { RightSidebar } from "@/right-panel/RightSidebar";
import { useRightPanelStore } from "@/right-panel/right-panel-store";
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
import { useWorkspaceStore } from "@/workspace/workspace-store";
import { NavigationSidebar } from "@/navigation/NavigationSidebar";
import { useNavigationStore } from "@/navigation/navigation-store";
import { SettingsDialog } from "@/settings/SettingsDialog";

/** Below this width the sidebar becomes a drawer so the terminal keeps the room. */
const WIDE_LAYOUT_QUERY = "(min-width: 900px)";
/** The right panel only docks beside the terminal when there is room for both. */
const DOCK_RIGHT_QUERY = "(min-width: 1280px)";
const REFRESH_ON_FOCUS_MS = 2000;

function App() {
  const wide = useMediaQuery(WIDE_LAYOUT_QUERY);
  const panelCollapsed = useNavigationStore((state) => state.panelCollapsed);
  const drawerOpen = useNavigationStore((state) => state.drawerOpen);
  const setDrawerOpen = useNavigationStore((state) => state.setDrawerOpen);

  const toggleSidebar = useCallback(() => {
    useNavigationStore.getState().togglePanel(wide);
  }, [wide]);

  const canDockRight = useMediaQuery(DOCK_RIGHT_QUERY);
  const rightDocked = useRightPanelStore((state) => state.dockedOpen);
  const rightDrawerOpen = useRightPanelStore((state) => state.drawerOpen);
  const setRightDrawerOpen = useRightPanelStore((state) => state.setDrawerOpen);
  const rightPanelOpen = canDockRight ? rightDocked : rightDrawerOpen;

  const toggleRightPanel = useCallback(() => {
    useRightPanelStore.getState().toggle(canDockRight);
  }, [canDockRight]);
  // The shortcut always lands on Files: open it if the panel is closed, close it if it is open.
  const toggleFilesPanel = useCallback(() => {
    const panel = useRightPanelStore.getState();
    const open = canDockRight ? panel.dockedOpen : panel.drawerOpen;
    if (open && panel.activeTab === "files") panel.toggle(canDockRight);
    else panel.show("files", canDockRight);
  }, [canDockRight]);
  useAppShortcuts(toggleSidebar, toggleFilesPanel);

  useEffect(() => {
    Promise.all([
      useWorkspaceStore.getState().load(),
      useTerminalStore.getState().load(),
      useConfigStore.getState().load(),
    ]).catch((error: unknown) => {
      toast.error(copy.sidebar.loadError, { description: errorMessage(error) });
    });

    const unlistenExit = events.terminalExit.listen(({ payload }) => {
      useTerminalStore.getState().markExited(payload.id, payload.exitCode);
    });

    // The config file is meant to be edited by hand, so changes can arrive at any time.
    const unlistenConfig = events.configChanged.listen(({ payload }) => {
      useConfigStore.getState().apply(payload.snapshot);
    });

    // Worktrees can change outside the app; coming back to the window is when it matters.
    let lastRefresh = 0;
    const refreshOnFocus = () => {
      if (Date.now() - lastRefresh < REFRESH_ON_FOCUS_MS) return;
      lastRefresh = Date.now();
      void useWorkspaceStore.getState().refreshAll();
      const selected = useWorkspaceStore.getState().selectedWorktreePath;
      if (selected && useExplorerStore.getState().trees[selected]) {
        void useExplorerStore.getState().refresh(selected);
      }
    };
    window.addEventListener("focus", refreshOnFocus);

    return () => {
      window.removeEventListener("focus", refreshOnFocus);
      void unlistenExit.then((unlisten) => unlisten());
      void unlistenConfig.then((unlisten) => unlisten());
    };
  }, []);

  const panelVisible = wide && !panelCollapsed;

  return (
    <TooltipProvider delayDuration={400}>
      <div className="flex h-full bg-background text-foreground">
        {wide && <NavigationSidebar wide panelVisible={panelVisible} />}
        <TerminalWorkspace
          sidebarVisible={panelVisible}
          onToggleSidebar={toggleSidebar}
          rightPanelOpen={rightPanelOpen}
          onToggleRightPanel={toggleRightPanel}
        />
        {canDockRight && rightDocked && (
          <aside
            aria-label={copy.rightPanel.label}
            className="w-72 shrink-0 border-l border-sidebar-border"
          >
            <RightSidebar />
          </aside>
        )}
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
            className="gap-0 p-0 data-[side=left]:w-75 data-[side=left]:sm:max-w-75"
            showCloseButton={false}
          >
            <SheetTitle className="sr-only">{copy.sidebar.title}</SheetTitle>
            <NavigationSidebar
              wide={false}
              panelVisible
              onNavigate={() => setDrawerOpen(false)}
            />
          </SheetContent>
        </Sheet>
      )}

      {!canDockRight && (
        <Sheet open={rightDrawerOpen} onOpenChange={setRightDrawerOpen}>
          <SheetContent
            side="right"
            showCloseButton={false}
            onOpenAutoFocus={(event) => {
              // Land in the tree, not on the first button (which would pop its tooltip).
              event.preventDefault();
              document
                .querySelector<HTMLElement>(
                  '[data-slot="sheet-content"] [role="tree"] [tabindex="0"]',
                )
                ?.focus();
            }}
            className="gap-0 p-0 data-[side=right]:w-80 data-[side=right]:sm:max-w-80"
          >
            <SheetTitle className="sr-only">{copy.rightPanel.label}</SheetTitle>
            <RightSidebar />
          </SheetContent>
        </Sheet>
      )}

      <SettingsDialog />
      <NewWorktreeDialog />
      <RemoveWorktreeDialog />
      <RemoveRepoDialog />
      <CustomCommandDialog />
      <Toaster position="bottom-right" />
    </TooltipProvider>
  );
}

export default App;
