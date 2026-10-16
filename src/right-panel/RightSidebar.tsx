import { TooltipIconButton } from "@/components/TooltipIconButton";
import { cn } from "@/lib/utils";
import { RIGHT_PANEL_TABS } from "@/right-panel/right-panel-tabs";
import { useRightPanelStore } from "@/right-panel/right-panel-store";
import { copy } from "@/ui-copy";

/** The right-hand panel: a tab bar on top and the chosen tab's content below. */
export function RightSidebar() {
  const activeTab = useRightPanelStore((state) => state.activeTab);
  const selectTab = useRightPanelStore((state) => state.selectTab);
  const tab =
    RIGHT_PANEL_TABS.find((candidate) => candidate.id === activeTab) ??
    RIGHT_PANEL_TABS[0];
  const Panel = tab.panel;

  return (
    <div className="flex h-full flex-col bg-sidebar text-sidebar-foreground">
      <div
        role="tablist"
        aria-label={copy.rightPanel.tabs}
        className="flex h-9 shrink-0 items-center gap-1 px-1.5"
      >
        {RIGHT_PANEL_TABS.map((candidate) => (
          <TooltipIconButton
            key={candidate.id}
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
