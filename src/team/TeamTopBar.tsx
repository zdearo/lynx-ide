import { ChevronDownGlyph18 } from "@/components/icons/ChevronDownGlyph18";
import { Button } from "@/components/ui/button";
import {
  DropdownMenu,
  DropdownMenuCheckboxItem,
  DropdownMenuContent,
  DropdownMenuItem,
  DropdownMenuSeparator,
  DropdownMenuTrigger,
} from "@/components/ui/dropdown-menu";
import { ExpandGlyph18 } from "@/components/icons/ExpandGlyph18";
import { PlusGlyph18 } from "@/components/icons/PlusGlyph18";
import { SidebarLeftGlyph18 } from "@/components/icons/SidebarLeftGlyph18";
import { SidebarRightGlyph18 } from "@/components/icons/SidebarRightGlyph18";
import { TerminalGlyph18 } from "@/components/icons/TerminalGlyph18";
import { TooltipIconButton } from "@/components/TooltipIconButton";
import { shortcutLabel } from "@/platform-shortcuts";
import { TeamSummary } from "@/team/TeamStatus";
import { useRunMembers, useTeamStore } from "@/team/team-store";
import { copy } from "@/ui-copy";

/** The bar above the team, in place of the terminal tabs: the run, its summary and the team's actions. */
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
  const members = useRunMembers();
  const runs = useTeamStore((state) => state.runs);
  const runId = useTeamStore((state) => state.runId);
  const run = runs.find((item) => item.id === runId);

  return (
    <div className="flex h-9 shrink-0 items-center gap-2 bg-sidebar px-1.5">
      <div className="flex items-center">
        <TooltipIconButton
          label={copy.sidebar.toggle}
          shortcut={shortcutLabel("toggleSidebar")}
          onClick={onToggleSidebar}
        >
          <SidebarLeftGlyph18 />
        </TooltipIconButton>
        {/* With the sidebar hidden, nothing else on screen leads back to the terminals. */}
        {!sidebarVisible && (
          <TooltipIconButton
            label={copy.team.mode.terminal}
            onClick={() => {
              if (path) useTeamStore.getState().setMode(path, "terminal");
            }}
          >
            <TerminalGlyph18 />
          </TooltipIconButton>
        )}
      </div>
      <div className="flex min-w-0 items-center gap-2 pl-1">
        <DropdownMenu>
          <DropdownMenuTrigger asChild>
            <Button
              variant="ghost"
              size="sm"
              aria-label={copy.team.runs.label}
              className="min-w-0 gap-1 px-1.5 font-medium"
            >
              <span className="truncate">
                {run?.title ?? copy.team.runs.none}
              </span>
              <ChevronDownGlyph18 className="size-3 shrink-0" />
            </Button>
          </DropdownMenuTrigger>
          <DropdownMenuContent align="start" className="min-w-48">
            {runs.map((item) => (
              <DropdownMenuCheckboxItem
                key={item.id}
                checked={item.id === runId}
                onCheckedChange={() => useTeamStore.getState().setRun(item.id)}
              >
                {item.title}
              </DropdownMenuCheckboxItem>
            ))}
            <DropdownMenuSeparator />
            <DropdownMenuItem
              onSelect={() => useTeamStore.getState().setNewRunOpen(true)}
            >
              {copy.team.runs.newRun}
            </DropdownMenuItem>
            {run && (
              <>
                <DropdownMenuSeparator />
                <DropdownMenuItem
                  variant="destructive"
                  onSelect={() =>
                    useTeamStore.getState().setDeleteRunOpen(true)
                  }
                >
                  {copy.team.runs.deleteItem}
                </DropdownMenuItem>
              </>
            )}
          </DropdownMenuContent>
        </DropdownMenu>
        <TeamSummary members={members} />
      </div>

      <div className="ml-auto flex items-center gap-0.5">
        <TooltipIconButton
          label={copy.team.recruit}
          disabled={!runId}
          onClick={() => useTeamStore.getState().setAddOpen(true)}
        >
          <PlusGlyph18 />
        </TooltipIconButton>
        <TooltipIconButton
          label={copy.team.fit}
          onClick={() => useTeamStore.getState().fitMap?.()}
        >
          <ExpandGlyph18 />
        </TooltipIconButton>
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
