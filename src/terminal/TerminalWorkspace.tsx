import { FolderOpen } from "lucide-react";

import { Button } from "@/components/ui/button";
import { useDialogStore } from "@/dialog-store";
import { shortcutLabel } from "@/platform-shortcuts";
import { TERMINAL_PRESETS } from "@/terminal/agent-presets";
import { PresetIcon } from "@/terminal/PresetIcon";
import { startTerminal } from "@/terminal/terminal-actions";
import { TerminalTabs } from "@/terminal/TerminalTabs";
import { activeSessionId, useTerminalStore } from "@/terminal/terminal-store";
import { TerminalView } from "@/terminal/TerminalView";
import { copy } from "@/ui-copy";
import { openRepositoryFromPicker } from "@/workspace/open-repository";
import {
  useSelectedWorktree,
  type SelectedWorktree,
} from "@/workspace/use-selected-worktree";
import { worktreeLabel } from "@/workspace/worktree-label";
import { useWorkspaceStore } from "@/workspace/workspace-store";

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

  return (
    <main className="flex min-w-0 flex-1 flex-col bg-background">
      <TerminalTabs
        showLocation={!sidebarVisible}
        onToggleSidebar={onToggleSidebar}
        rightPanelOpen={rightPanelOpen}
        onToggleRightPanel={onToggleRightPanel}
      />
      <div className="relative min-h-0 flex-1">
        {/* Every session stays mounted so switching worktrees keeps scroll position and screen state. */}
        {sessions.map((session) => (
          <div key={session.id} className="absolute inset-0">
            <TerminalView
              terminalId={session.id}
              active={session.id === activeId}
            />
          </div>
        ))}
        {loaded && repoCount === 0 && <Welcome />}
        {loaded && selected && !hasSessions && (
          <EmptyWorktree selected={selected} />
        )}
      </div>
    </main>
  );
}

function Welcome() {
  return (
    <div className="absolute inset-0 flex flex-col items-center justify-center gap-4 px-6 text-center">
      <div className="max-w-sm">
        <h2 className="text-sm font-medium">{copy.welcome.title}</h2>
        <p className="mt-1 text-ui text-muted-foreground">
          {copy.welcome.body}
        </p>
      </div>
      <Button onClick={() => void openRepositoryFromPicker()}>
        <FolderOpen />
        {copy.sidebar.openRepository}
      </Button>
    </div>
  );
}

function EmptyWorktree({ selected }: { selected: SelectedWorktree }) {
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
        {TERMINAL_PRESETS.map((preset) => (
          <Button
            key={preset.id}
            variant="outline"
            size="sm"
            onClick={() => void startTerminal(path, preset.command)}
          >
            <PresetIcon kind={preset.kind} />
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
