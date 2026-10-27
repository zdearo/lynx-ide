import { lazy, Suspense, useCallback, useEffect } from "react";
import { toast } from "sonner";

import { events } from "@/bindings";
import { useBrowserStore } from "@/browser/browser-store";
import { useAgentCatalogStore } from "@/agents/agent-catalog-store";
import { DialogHost } from "@/dialog-host";
import { useBotChatStore } from "@/bots/bot-chat-store";
import { useChatGptAuthStore } from "@/bots/chatgpt-auth-store";
import { useConfigStore } from "@/config/config-store";
import { useExplorerStore } from "@/files/explorer-store";
import { useRightPanelStore } from "@/right-panel/right-panel-store";
import { Toaster } from "@/components/ui/sonner";
import { Sheet, SheetContent, SheetTitle } from "@/components/ui/sheet";
import { TooltipProvider } from "@/components/ui/tooltip";
import { DOCK_RIGHT_QUERY, WIDE_LAYOUT_QUERY } from "@/hooks/layout-queries";
import { useMediaQuery } from "@/hooks/use-media-query";
import { errorMessage } from "@/lib/error-message";
import { handleTerminalExit } from "@/terminal/terminal-actions";
import { useTerminalStore } from "@/terminal/terminal-store";

// The side panel brings its file explorer with it, so it loads when first shown.
const RightSidebar = lazy(() =>
  import("@/right-panel/RightSidebar").then((m) => ({
    default: m.RightSidebar,
  })),
);
import { TerminalWorkspace } from "@/terminal/TerminalWorkspace";
import { StatusBar } from "@/status-bar/StatusBar";
import { copy } from "@/ui-copy";
import { notifyAgentStatus } from "@/notifications/notify-agent-status";
import { notifyUpdate } from "@/updates/notify-update";
import { useUpdateStore } from "@/updates/update-store";
import { applyWorktreeChange } from "@/watching/apply-worktree-change";
import { useWorktreeWatch } from "@/watching/use-worktree-watch";
import { useLinkedFolderSync } from "@/workspace/projects/use-linked-folder-sync";
import { useAppShortcuts } from "@/use-app-shortcuts";
import { useWorkspaceStore } from "@/workspace/workspace-store";
import {
  NavigationPanel,
  NavigationRail,
  NavigationSidebar,
} from "@/navigation/NavigationSidebar";
import { useNavigationStore } from "@/navigation/navigation-store";
import { useQuitGuard } from "@/quit-guard";

const REFRESH_ON_FOCUS_MS = 2000;

function App() {
  const selectedWorktreePath = useWorkspaceStore(
    (state) => state.selectedWorktreePath,
  );
  useWorktreeWatch(selectedWorktreePath);
  useLinkedFolderSync();
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
  useQuitGuard();

  useEffect(() => {
    Promise.all([
      useWorkspaceStore.getState().load(),
      useTerminalStore.getState().load(),
      useConfigStore.getState().load(),
      useAgentCatalogStore.getState().load(),
      useUpdateStore.getState().load(),
      useChatGptAuthStore.getState().load(),
    ]).catch((error: unknown) => {
      toast.error(copy.sidebar.loadError, { description: errorMessage(error) });
    });

    const unlistenExit = events.terminalExit.listen(({ payload }) => {
      handleTerminalExit(payload.id, payload.exitCode);
    });

    // A session an agent started through the backend appears in the list, not only one the screen made.
    const unlistenCreated = events.terminalCreated.listen(({ payload }) => {
      useTerminalStore.getState().added(payload.session);
    });

    const unlistenAgentStatus = events.agentStatusChanged.listen(
      ({ payload }) => {
        // Before the store takes the new status, so the notification can compare with the old one.
        void notifyAgentStatus(payload);
        useTerminalStore
          .getState()
          .setAgentState(payload.id, payload.harness, payload.status);
      },
    );

    // An agent or another program wrote files in the worktree on screen.
    const unlistenWorktree = events.worktreeChanged.listen(({ payload }) => {
      applyWorktreeChange(payload);
    });

    // An agent made a worktree: the sidebar lists it only once the screen reads the project again.
    const unlistenWorktreeCreated = events.worktreeCreated.listen(
      ({ payload }) => {
        void useWorkspaceStore.getState().refreshWorktrees(payload.repoId);
      },
    );

    // The config file is meant to be edited by hand, so changes can arrive at any time.
    const unlistenBrowser = events.browserStateChanged.listen(({ payload }) => {
      // A tab in its own window reports to that window. The main view only hears about the one it
      // has selected, to keep saying that it is in a window.
      const { state } = useBrowserStore.getState();
      if (!payload.state.detached || payload.state.page === state.page)
        useBrowserStore.getState().apply(payload.state);
    });

    const unlistenAgentBrowser = events.browserAgentActivity.listen(
      ({ payload }) => {
        useBrowserStore
          .getState()
          .noteAgentAction(payload.page, payload.agent, payload.summary);
      },
    );

    const unlistenPages = events.browserPagesChanged.listen(({ payload }) => {
      useBrowserStore.getState().applyPages(payload.pages);
    });
    const unlistenThumbnails = events.browserThumbnailChanged.listen(
      ({ payload }) => {
        useBrowserStore.getState().applyThumbnail(payload.page, payload.png);
      },
    );
    void useBrowserStore.getState().loadPages();

    const unlistenAgentReleased = events.browserAgentReleased.listen(() => {
      useBrowserStore.getState().releaseAgent();
    });

    const unlistenEngine = events.browserEngineProgress.listen(
      ({ payload }) => {
        useBrowserStore.getState().applyEngineProgress(payload.progress);
      },
    );

    const unlistenChatGpt = events.chatGptAuthChanged.listen(({ payload }) => {
      useChatGptAuthStore.getState().apply(payload.status);
    });

    // A bot's reply comes in pieces while it is written; the chat shows them as they arrive.
    // A channel reports its status, and each message it saves, as it goes.
    const unlistenChannelStatus = events.channelStatus.listen(({ payload }) => {
      useBotChatStore.getState().status(payload.thread, {
        phase: payload.phase,
        author: payload.author,
        waiting: payload.waiting,
      });
    });
    const unlistenMessageAdded = events.chatMessageAdded.listen(
      ({ payload }) => {
        useBotChatStore.getState().added(payload.thread, payload.message);
      },
    );

    const unlistenBotReply = events.botReplyDelta.listen(({ payload }) => {
      useBotChatStore
        .getState()
        .delta(payload.thread, payload.author, payload.text);
    });

    const unlistenConfig = events.configChanged.listen(({ payload }) => {
      useConfigStore.getState().apply(payload.snapshot);
    });

    // The update check runs in the background; a new version is announced when it is found.
    const unlistenUpdate = events.updateStatusChanged.listen(({ payload }) => {
      notifyUpdate(payload.status);
      useUpdateStore.getState().apply(payload.status);
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
      void unlistenCreated.then((unlisten) => unlisten());
      void unlistenAgentStatus.then((unlisten) => unlisten());
      void unlistenWorktree.then((unlisten) => unlisten());
      void unlistenWorktreeCreated.then((unlisten) => unlisten());
      void unlistenChatGpt.then((unlisten) => unlisten());
      void unlistenBotReply.then((unlisten) => unlisten());
      void unlistenChannelStatus.then((unlisten) => unlisten());
      void unlistenMessageAdded.then((unlisten) => unlisten());
      void unlistenConfig.then((unlisten) => unlisten());
      void unlistenUpdate.then((unlisten) => unlisten());
      void unlistenBrowser.then((unlisten) => unlisten());
      void unlistenAgentBrowser.then((unlisten) => unlisten());
      void unlistenPages.then((unlisten) => unlisten());
      void unlistenThumbnails.then((unlisten) => unlisten());
      void unlistenAgentReleased.then((unlisten) => unlisten());
      void unlistenEngine.then((unlisten) => unlisten());
    };
  }, []);

  const panelVisible = wide && !panelCollapsed;

  return (
    <TooltipProvider delayDuration={400}>
      <div className="flex h-full bg-background text-foreground">
        {wide && <NavigationRail wide panelVisible={panelVisible} />}
        <div className="flex min-h-0 min-w-0 flex-1 flex-col">
          <div className="flex min-h-0 flex-1">
            {wide && panelVisible && <NavigationPanel />}
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
                <Suspense fallback={null}>
                  <RightSidebar />
                </Suspense>
              </aside>
            )}
          </div>
          <StatusBar />
        </div>
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
            <Suspense fallback={null}>
              <RightSidebar />
            </Suspense>
          </SheetContent>
        </Sheet>
      )}

      <DialogHost />
      <Toaster position="bottom-right" />
    </TooltipProvider>
  );
}

export default App;
