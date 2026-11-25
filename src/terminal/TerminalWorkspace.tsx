import { lazy, Suspense, useEffect } from "react";

import { FolderSimpleIcon } from "@phosphor-icons/react";

import { Button } from "@/components/ui/button";
import { BotArea, BotTopBar } from "@/bots/BotMain";
import {
  selectedBotOf,
  selectedChannelOf,
  useBotsStore,
} from "@/bots/bots-store";
import { useDialogStore } from "@/dialog-store";
import { useNavigationStore } from "@/navigation/navigation-store";
import { shortcutLabel } from "@/platform-shortcuts";
import { BrowserView } from "@/browser/BrowserView";
import { useBrowserStore } from "@/browser/browser-store";
import { FileTabs } from "@/editor/FileTabs";
import { activeFileOf, useEditorStore } from "@/editor/editor-store";
import { useVisiblePresets } from "@/agents/agent-catalog-store";
import { PresetIcon } from "@/terminal/PresetIcon";
import { startPreset } from "@/terminal/terminal-actions";
import { TerminalTabs } from "@/terminal/TerminalTabs";
import { activeSessionId, useTerminalStore } from "@/terminal/terminal-store";
import { TerminalView } from "@/terminal/TerminalView";
import { TeamTopBar } from "@/team/TeamTopBar";
import { teamModeOf, useTeamStore } from "@/team/team-store";
import { copy } from "@/ui-copy";
import {
  useSelectedWorktree,
  type SelectedWorktree,
} from "@/workspace/worktrees/use-selected-worktree";
import { worktreeLabel } from "@/workspace/worktrees/worktree-label";
import { useWorkspaceStore } from "@/workspace/workspace-store";

// CodeMirror loads the first time a file opens, not at startup.
const FileEditor = lazy(() =>
  import("@/editor/FileEditor").then((m) => ({ default: m.FileEditor })),
);
// The team map is React Flow, loaded only when a worktree is switched to the team.
const TeamView = lazy(() =>
  import("@/team/TeamView").then((m) => ({ default: m.TeamView })),
);

interface TerminalWorkspaceProps {
  sidebarVisible: boolean;
  onToggleSidebar: () => void;
  rightPanelOpen: boolean;
  onToggleRightPanel: () => void;
}

export function TerminalWorkspace({
  sidebarVisible,
  onToggleSidebar,
  rightPanelOpen,
  onToggleRightPanel,
}: TerminalWorkspaceProps) {
  const loaded = useWorkspaceStore((state) => state.loaded);
  const repoCount = useWorkspaceStore((state) => state.repos.length);
  const selected = useSelectedWorktree();
  const sessions = useTerminalStore((state) => state.sessions);
  const activeByWorktree = useTerminalStore((state) => state.activeByWorktree);

  const path = selected?.worktree.path ?? null;
  const activeId = activeSessionId(sessions, activeByWorktree, path);
  const hasSessions = sessions.some((session) => session.worktreePath === path);
  const openFile = useEditorStore((state) => activeFileOf(state, path));
  const browserOpen = useBrowserStore((state) => state.open);
  const teamMode =
    useTeamStore((state) => teamModeOf(state.modeByWorktree, path)) === "team";
  // In the Bots view the main area is the chat with the picked bot; the terminals stay mounted, hidden.
  const botsView = useNavigationStore((state) => state.activeView === "bots");
  const bot = useBotsStore(selectedBotOf);
  const channel = useBotsStore(selectedChannelOf);

  // Looking at a finished agent is what clears its "done" mark.
  const activeFinished = sessions.some(
    (session) => session.id === activeId && session.agentStatus === "done",
  );
  useEffect(() => {
    if (activeFinished && activeId) {
      useTerminalStore.getState().markSeen(activeId);
    }
  }, [activeFinished, activeId]);

  // A file or the browser coming in front takes the team away; the team takes the file and the browser away.
  useEffect(() => {
    if ((openFile || browserOpen) && path) {
      useTeamStore.getState().setMode(path, "terminal");
    }
  }, [openFile, browserOpen, path]);
  useEffect(() => {
    if (!teamMode || !path) return;
    useBrowserStore.getState().setOpen(false);
    useEditorStore.getState().show(path, null);
  }, [teamMode, path]);

  return (
    <main className="relative flex min-w-0 flex-1 flex-col bg-background">
      {botsView ? (
        <BotTopBar
          bot={bot}
          channel={channel}
          rightPanelOpen={rightPanelOpen}
          onToggleSidebar={onToggleSidebar}
          onToggleRightPanel={onToggleRightPanel}
        />
      ) : teamMode ? (
        <TeamTopBar
          path={path}
          sidebarVisible={sidebarVisible}
          rightPanelOpen={rightPanelOpen}
          onToggleSidebar={onToggleSidebar}
          onToggleRightPanel={onToggleRightPanel}
        />
      ) : (
        <TerminalTabs
          showLocation={!sidebarVisible}
          onToggleSidebar={onToggleSidebar}
          rightPanelOpen={rightPanelOpen}
          onToggleRightPanel={onToggleRightPanel}
        />
      )}
      {!botsView && !teamMode && path && <FileTabs worktreePath={path} />}
      <div className="relative min-h-0 flex-1">
        {/* Every session stays mounted so switching worktrees keeps scroll position and screen state. In team mode the
            team's cards draw their terminals instead, so a session has one view at a time. */}
        {!teamMode &&
          sessions.map((session) => {
            const shown =
              session.id === activeId && !openFile && !browserOpen && !botsView;
            return (
              // A hidden session must leave the layout too: its box would still take the clicks meant for the visible one.
              <div
                key={session.id}
                className={
                  shown ? "absolute inset-0" : "absolute inset-0 invisible"
                }
              >
                <TerminalView terminalId={session.id} active={shown} />
              </div>
            );
          })}
        {botsView && <BotArea />}
        {!botsView && loaded && repoCount === 0 && <Welcome />}
        {!botsView && path && openFile && (
          <div className="absolute inset-0 bg-background">
            <Suspense fallback={null}>
              <FileEditor worktreePath={path} file={openFile} />
            </Suspense>
          </div>
        )}
        {!botsView && teamMode && (
          <div className="absolute inset-0 bg-background">
            <Suspense fallback={null}>
              <TeamView />
            </Suspense>
          </div>
        )}
        {!botsView &&
          loaded &&
          selected &&
          !hasSessions &&
          !openFile &&
          !browserOpen &&
          !teamMode && <EmptyWorktree selected={selected} />}
        {!botsView && browserOpen && <BrowserView />}
      </div>
    </main>
  );
}

function Welcome() {
  const askAddProject = useDialogStore((state) => state.askAddProject);

  return (
    <div className="absolute inset-0 flex flex-col items-center justify-center gap-4 px-6 text-center">
      <div className="max-w-sm">
        <h2 className="text-sm font-medium">{copy.welcome.title}</h2>
        <p className="mt-1 text-ui text-muted-foreground">
          {copy.welcome.body}
        </p>
      </div>
      <Button onClick={() => askAddProject(true)}>
        <FolderSimpleIcon />
        {copy.sidebar.addProject}
      </Button>
    </div>
  );
}

function EmptyWorktree({ selected }: { selected: SelectedWorktree }) {
  const presets = useVisiblePresets();
  const askCustomCommand = useDialogStore((state) => state.askCustomCommand);
  const path = selected.worktree.path;

  return (
    <div className="absolute inset-0 flex flex-col items-center justify-center gap-4 px-6 text-center">
      <div className="max-w-sm">
        <h2 className="text-sm font-medium">
          {copy.emptyWorktree.title(worktreeLabel(selected.worktree).text)}
        </h2>
        <p className="mt-1 text-ui text-muted-foreground">
          {copy.emptyWorktree.body}
        </p>
      </div>
      <div className="flex flex-wrap justify-center gap-2">
        {presets.map((preset) => (
          <Button
            key={preset.id}
            variant="outline"
            size="sm"
            onClick={() => void startPreset(path, preset)}
          >
            <PresetIcon harness={preset.harness} />
            {preset.label}
          </Button>
        ))}
        <Button
          variant="ghost"
          size="sm"
          onClick={() => askCustomCommand(path)}
        >
          {copy.terminal.customCommand}
        </Button>
      </div>
      <p className="text-meta text-muted-foreground tabular-nums">
        {shortcutLabel("newTerminal")} · {copy.shortcuts.newTerminal}
      </p>
    </div>
  );
}
