import { lazy, Suspense, useEffect } from "react";

import { Folder2Glyph18 } from "@/components/icons/Folder2Glyph18";

import { Button } from "@/components/ui/button";
import { useDialogStore } from "@/dialog-store";
import { shortcutLabel } from "@/platform-shortcuts";
import { FileTabs } from "@/editor/FileTabs";
import { activeFileOf, useEditorStore } from "@/editor/editor-store";
import { useVisiblePresets } from "@/agents/agent-catalog-store";
import { PresetIcon } from "@/terminal/PresetIcon";
import { startPreset } from "@/terminal/terminal-actions";
import { TerminalTabs } from "@/terminal/TerminalTabs";
import { activeSessionId, useTerminalStore } from "@/terminal/terminal-store";
import { TerminalView } from "@/terminal/TerminalView";
import { cn } from "@/lib/utils";
import { copy } from "@/ui-copy";
import {
  useSelectedWorktree,
  type SelectedWorktree,
} from "@/workspace/use-selected-worktree";
import { worktreeLabel } from "@/workspace/worktree-label";
import { useWorkspaceStore } from "@/workspace/workspace-store";

// CodeMirror loads the first time a file opens, not at startup.
const FileEditor = lazy(() =>
  import("@/editor/FileEditor").then((m) => ({ default: m.FileEditor })),
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

  // Looking at a finished agent is what clears its "done" mark.
  const activeFinished = sessions.some(
    (session) => session.id === activeId && session.agentStatus === "done",
  );
  useEffect(() => {
    if (activeFinished && activeId) {
      useTerminalStore.getState().markSeen(activeId);
    }
  }, [activeFinished, activeId]);

  return (
    <main className="flex min-w-0 flex-1 flex-col bg-background">
      <TerminalTabs
        showLocation={!sidebarVisible}
        onToggleSidebar={onToggleSidebar}
        rightPanelOpen={rightPanelOpen}
        onToggleRightPanel={onToggleRightPanel}
      />
      {path && <FileTabs worktreePath={path} />}
      <div className="relative min-h-0 flex-1">
        {/* Every session stays mounted so switching worktrees keeps scroll position and screen state. */}
        {sessions.map((session) => (
          // A hidden session must leave the layout too: its box would still take the clicks meant for the visible one.
          <div
            key={session.id}
            className={cn(
              "absolute inset-0",
              (session.id !== activeId || openFile) && "hidden",
            )}
          >
            <TerminalView
              terminalId={session.id}
              active={session.id === activeId && !openFile}
            />
          </div>
        ))}
        {loaded && repoCount === 0 && <Welcome />}
        {path && openFile && (
          <div className="absolute inset-0 bg-background">
            <Suspense fallback={null}>
              <FileEditor worktreePath={path} file={openFile} />
            </Suspense>
          </div>
        )}
        {loaded && selected && !hasSessions && !openFile && (
          <EmptyWorktree selected={selected} />
        )}
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
        <Folder2Glyph18 />
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
