import {
  ArrowsOutIcon,
  PlusIcon,
  SidebarSimpleIcon,
  TerminalIcon,
} from "@phosphor-icons/react";
import { TooltipIconButton } from "@/components/TooltipIconButton";
import { shortcutLabel } from "@/platform-shortcuts";
import { TeamSummary } from "@/team/TeamStatus";
import { useTeamMembers, useTeamStore } from "@/team/team-store";
import { copy } from "@/ui-copy";
import { useSelectedWorktree } from "@/workspace/worktrees/use-selected-worktree";
import { worktreeLabel } from "@/workspace/worktrees/worktree-label";

/** The bar above the team, in place of the terminal tabs: the worktree, its summary and the team's actions. */
export function TeamTopBar({
  path,
  sidebarVisible,
  rightPanelOpen,
  onToggleSidebar,
  onToggleRightPanel,
}: {
  path: string | null;
  sidebarVisible: boolean;
  rightPanelOpen: boolean;
  onToggleSidebar: () => void;
  onToggleRightPanel: () => void;
}) {
  const members = useTeamMembers();
  const selected = useSelectedWorktree();

  return (
    <div className="flex h-9 shrink-0 items-center gap-2 bg-sidebar px-1.5">
      <div className="flex items-center">
        <TooltipIconButton
          label={copy.sidebar.toggle}
          shortcut={shortcutLabel("toggleSidebar")}
          onClick={onToggleSidebar}
        >
          <SidebarSimpleIcon />
        </TooltipIconButton>
        {/* With the sidebar hidden, nothing else on screen leads back to the terminals. */}
        {!sidebarVisible && (
          <TooltipIconButton
            label={copy.team.mode.terminal}
            onClick={() => {
              if (path) useTeamStore.getState().setMode(path, "terminal");
            }}
          >
            <TerminalIcon />
          </TooltipIconButton>
        )}
      </div>
      <div className="flex min-w-0 items-center gap-2 pl-1">
        {selected && (
          <span className="min-w-0 truncate font-medium">
            {worktreeLabel(selected.worktree).text}
          </span>
        )}
        <TeamSummary members={members} />
      </div>

      <div className="ml-auto flex items-center gap-0.5">
        <TooltipIconButton
          label={copy.team.recruit}
          disabled={!path}
          onClick={() => useTeamStore.getState().setAddOpen(true)}
        >
          <PlusIcon />
        </TooltipIconButton>
        <TooltipIconButton
          label={copy.team.fit}
          onClick={() => useTeamStore.getState().fitMap?.()}
        >
          <ArrowsOutIcon />
        </TooltipIconButton>
        <TooltipIconButton
          label={copy.rightPanel.toggle}
          shortcut={shortcutLabel("toggleFilesPanel")}
          aria-pressed={rightPanelOpen}
          onClick={onToggleRightPanel}
        >
          <SidebarSimpleIcon mirrored />
        </TooltipIconButton>
      </div>
    </div>
  );
}
