import { Fragment } from "react";
import { TooltipIconButton } from "@/components/TooltipIconButton";
import { cn } from "@/lib/utils";
import {
  BOT_RIGHT_PANEL_TABS,
  RIGHT_PANEL_TABS,
} from "@/right-panel/right-panel-tabs";
import { useRightPanelStore } from "@/right-panel/right-panel-store";
import { useNavigationStore } from "@/navigation/navigation-store";
import { copy } from "@/ui-copy";

/** The right-hand panel: a tab bar on top and the chosen tab's content below. */
export function RightSidebar() {
  const activeTab = useRightPanelStore((state) => state.activeTab);
  const selectTab = useRightPanelStore((state) => state.selectTab);
  const botsView = useNavigationStore((state) => state.activeView === "bots");
  const tabs = botsView ? BOT_RIGHT_PANEL_TABS : RIGHT_PANEL_TABS;
  // A tab from the other view falls back to this view's first tab.
  const tab = tabs.find((candidate) => candidate.id === activeTab) ?? tabs[0];
  const Panel = tab.panel;

  return (
    <div className="flex h-full flex-col bg-sidebar text-sidebar-foreground">
      <div
        role="tablist"
        aria-label={copy.rightPanel.tabs}
        className="flex h-9 shrink-0 items-center gap-1 px-1.5"
      >
        {tabs.map((candidate) => (
          <Fragment key={candidate.id}>
            {candidate.separatorBefore && (
              <span aria-hidden="true" className="mx-1 h-4 w-px bg-border" />
            )}
            <TooltipIconButton
              role="tab"
              id={`side-tab-${candidate.id}`}
              aria-selected={candidate.id === tab.id}
              aria-controls="side-panel"
              label={candidate.label}
              onClick={() => selectTab(candidate.id)}
              className={cn(
                "size-7",
                candidate.id === tab.id
                  ? "bg-sidebar-accent text-foreground"
                  : "text-muted-foreground hover:text-foreground",
              )}
            >
              <candidate.icon className="size-4" />
            </TooltipIconButton>
          </Fragment>
        ))}
      </div>
      <div
        role="tabpanel"
        id="side-panel"
        aria-labelledby={`side-tab-${tab.id}`}
        className="min-h-0 flex-1"
      >
        <Panel />
      </div>
    </div>
  );
}
