import { useEffect, type KeyboardEvent } from "react";
import { useEditorStore } from "@/editor/editor-store";
import {
  ContextMenu,
  ContextMenuContent,
  ContextMenuItem,
  ContextMenuTrigger,
} from "@/components/ui/context-menu";
import { PlusGlyph18 } from "@/components/icons/PlusGlyph18";
import { SidebarLeftGlyph18 } from "@/components/icons/SidebarLeftGlyph18";
import { SidebarRightGlyph18 } from "@/components/icons/SidebarRightGlyph18";
import { XmarkGlyph18 } from "@/components/icons/XmarkGlyph18";

import { TooltipIconButton } from "@/components/TooltipIconButton";
import { useBrowserStore } from "@/browser/browser-store";
import { cn } from "@/lib/utils";
import { shortcutLabel } from "@/platform-shortcuts";
import { NewTerminalMenu } from "@/terminal/NewTerminalMenu";
import { PresetIcon } from "@/terminal/PresetIcon";
import { tabTitles } from "@/terminal/terminal-tab-title";
import {
  requestCloseTerminal,
  sleepTerminal,
} from "@/terminal/terminal-actions";
import { activeSessionId, useTerminalStore } from "@/terminal/terminal-store";
import { copy } from "@/ui-copy";
import { useSelectedWorktree } from "@/workspace/use-selected-worktree";
import { worktreeLabel } from "@/workspace/worktree-label";

interface TerminalTabsProps {
  /** When the sidebar is hidden the bar says which worktree this is. */
  showLocation: boolean;
  onToggleSidebar: () => void;
  rightPanelOpen: boolean;
  onToggleRightPanel: () => void;
}

export function TerminalTabs({
  showLocation,
  onToggleSidebar,
  rightPanelOpen,
  onToggleRightPanel,
}: TerminalTabsProps) {
  const selected = useSelectedWorktree();
  const allSessions = useTerminalStore((state) => state.sessions);
  const activeByWorktree = useTerminalStore((state) => state.activeByWorktree);
  const setActive = useTerminalStore((state) => state.setActive);

  // A terminal started from anywhere (the + menu, the shortcut) comes in front of the browser.
  useEffect(
    () =>
      useTerminalStore.subscribe((state, previous) => {
        if (state.sessions.length > previous.sessions.length) {
          useBrowserStore.getState().setOpen(false);
        }
      }),
    [],
  );

  const path = selected?.worktree.path ?? null;
  // While a file is on screen no terminal tab is the current one.
  const fileShown = useEditorStore((state) =>
    path ? Boolean(state.active[path]) : false,
  );
  const sessions = allSessions.filter(
    (session) => session.worktreePath === path,
  );
  const activeId = activeSessionId(allSessions, activeByWorktree, path);
  const titles = tabTitles(sessions);

  function moveTab(event: KeyboardEvent<HTMLDivElement>) {
    if (!path || (event.key !== "ArrowRight" && event.key !== "ArrowLeft"))
      return;
    const index = sessions.findIndex((session) => session.id === activeId);
    const step = event.key === "ArrowRight" ? 1 : -1;
    const next = sessions[(index + step + sessions.length) % sessions.length];
    if (!next) return;
    event.preventDefault();
    useBrowserStore.getState().setOpen(false);
    setActive(path, next.id);
    useEditorStore.getState().show(path, null);
    document.getElementById(`tab-${next.id}`)?.focus();
  }

  return (
    <div className="flex h-9 shrink-0 items-stretch bg-sidebar">
      <div className="flex items-center px-1.5">
        <TooltipIconButton
          label={copy.sidebar.toggle}
          shortcut={shortcutLabel("toggleSidebar")}
          onClick={onToggleSidebar}
        >
          <SidebarLeftGlyph18 />
        </TooltipIconButton>
      </div>

      {showLocation && selected && (
        <span
          className="flex max-w-40 min-w-0 shrink items-center truncate pr-2 text-meta text-muted-foreground"
          title={`${selected.repo.name} / ${worktreeLabel(selected.worktree).text}`}
        >
          {selected.repo.name} / {worktreeLabel(selected.worktree).text}
        </span>
      )}

      <div
        role="tablist"
        aria-label="Terminals"
        onKeyDown={moveTab}
        className="flex min-w-0 items-stretch overflow-x-auto"
      >
        {sessions.map((session) => {
          const active = session.id === activeId && !fileShown;
          return (
            // A tablist owns only tabs, so the close "x" lives inside the tab: a mouse
            // target here, and Delete (or the shortcut) for the keyboard.
            <ContextMenu key={session.id}>
              <ContextMenuTrigger asChild>
                <button
                  type="button"
                  role="tab"
                  id={`tab-${session.id}`}
                  aria-selected={active}
                  aria-keyshortcuts="Delete"
                  tabIndex={session.id === activeId ? 0 : -1}
                  onClick={(event) => {
                    if (
                      event.target instanceof Element &&
                      event.target.closest("[data-close]")
                    ) {
                      requestCloseTerminal(session.id);
                    } else if (path) {
                      useBrowserStore.getState().setOpen(false);
                      setActive(path, session.id);
                      useEditorStore.getState().show(path, null);
                    }
                  }}
                  onKeyDown={(event) => {
                    if (event.key === "Delete") {
                      event.preventDefault();
                      requestCloseTerminal(session.id);
                    }
                  }}
                  className={cn(
                    "group/tab flex h-full max-w-48 shrink-0 items-center gap-1.5 border-r border-border pr-1 pl-3 text-ui [&_svg]:size-3.5",
                    active
                      ? "bg-background text-foreground"
                      : "text-muted-foreground hover:text-foreground",
                  )}
                >
                  <PresetIcon harness={session.harness} />
                  <span className="truncate">{titles.get(session.id)}</span>
                  {!session.running && (
                    <span className="shrink-0 text-meta text-muted-foreground tabular-nums">
                      {copy.terminal.exited(session.exitCode)}
                    </span>
                  )}
                  {session.hibernated && (
                    <span className="shrink-0 text-meta text-muted-foreground">
                      {copy.terminal.sleeping}
                    </span>
                  )}
                  <span
                    data-close
                    aria-hidden
                    title={copy.terminal.close}
                    className={cn(
                      "grid size-5 shrink-0 place-items-center rounded-md hover:bg-muted",
                      !active &&
                        "opacity-0 group-focus-visible/tab:opacity-100 group-hover/tab:opacity-100",
                    )}
                  >
                    <XmarkGlyph18 />
                  </span>
                </button>
              </ContextMenuTrigger>
              <ContextMenuContent className="min-w-44">
                <ContextMenuItem
                  disabled={
                    !session.running ||
                    session.hibernated ||
                    session.agentStatus === "working"
                  }
                  onSelect={() => void sleepTerminal(session.id)}
                >
                  {copy.terminal.sleep}
                </ContextMenuItem>
              </ContextMenuContent>
            </ContextMenu>
          );
        })}
      </div>

      {selected && (
        <div className="flex items-center px-1.5">
          <NewTerminalMenu worktreePath={selected.worktree.path}>
            <TooltipIconButton
              label={copy.terminal.newTerminal}
              shortcut={shortcutLabel("newTerminal")}
            >
              <PlusGlyph18 />
            </TooltipIconButton>
          </NewTerminalMenu>
        </div>
      )}
      <div className="ml-auto flex items-center gap-0.5 px-1.5">
        <TooltipIconButton
          label={copy.rightPanel.toggle}
          shortcut={shortcutLabel("toggleFilesPanel")}
          aria-pressed={rightPanelOpen}
          onClick={onToggleRightPanel}
        >
          <SidebarRightGlyph18 />
        </TooltipIconButton>
      </div>
    </div>
  );
}
