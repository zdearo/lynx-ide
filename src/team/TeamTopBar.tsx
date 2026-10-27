import { toast } from "sonner";

import { BulletListGlyph18 } from "@/components/icons/BulletListGlyph18";
import { ExpandGlyph18 } from "@/components/icons/ExpandGlyph18";
import { MapGlyph18 } from "@/components/icons/MapGlyph18";
import { PlusGlyph18 } from "@/components/icons/PlusGlyph18";
import { SidebarLeftGlyph18 } from "@/components/icons/SidebarLeftGlyph18";
import { SidebarRightGlyph18 } from "@/components/icons/SidebarRightGlyph18";
import { TerminalGlyph18 } from "@/components/icons/TerminalGlyph18";
import { TooltipIconButton } from "@/components/TooltipIconButton";
import { TEAM_MAP_QUERY } from "@/hooks/layout-queries";
import { useMediaQuery } from "@/hooks/use-media-query";
import { shortcutLabel } from "@/platform-shortcuts";
import { TeamSummary } from "@/team/TeamTree";
import { TEAM_RUN_TITLE } from "@/team/team-fixtures";
import { useTeamStore } from "@/team/team-store";
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
  const members = useTeamStore((state) => state.members);
  const chosen = useTeamStore((state) => state.layout);
  // The tree takes over a narrow window whatever was chosen; the choice comes back when it widens.
  const wide = useMediaQuery(TEAM_MAP_QUERY);
  const layout = wide ? chosen : "list";

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
        <span className="truncate text-ui font-medium">{TEAM_RUN_TITLE}</span>
        <TeamSummary members={members} />
      </div>

      <div className="ml-auto flex items-center gap-0.5">
        <TooltipIconButton
          label={copy.team.recruit}
          onClick={() => toast.info(copy.team.recruitNotWired)}
        >
          <PlusGlyph18 />
        </TooltipIconButton>
        <TooltipIconButton
          label={copy.team.fit}
          disabled={layout !== "map"}
          onClick={() => useTeamStore.getState().fitMap?.()}
        >
          <ExpandGlyph18 />
        </TooltipIconButton>
        <TooltipIconButton
          label={layout === "map" ? copy.team.asList : copy.team.asMap}
          disabled={!wide}
          onClick={() =>
            useTeamStore.getState().setLayout(layout === "map" ? "list" : "map")
          }
        >
          {layout === "map" ? <BulletListGlyph18 /> : <MapGlyph18 />}
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
