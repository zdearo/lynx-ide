import { ActivityBar } from "@/navigation/ActivityBar";
import { useNavigationStore } from "@/navigation/navigation-store";
import { NAVIGATION_VIEWS } from "@/navigation/navigation-views";

interface NavigationSidebarProps {
  wide: boolean;
  panelVisible: boolean;
  /** Called after a choice in the panel, so a drawer can close itself. */
  onNavigate?: () => void;
}

/** The primary bar plus the side panel of whichever view it has selected. */
export function NavigationSidebar({
  wide,
  panelVisible,
  onNavigate,
}: NavigationSidebarProps) {
  const activeView = useNavigationStore((state) => state.activeView);
  const view =
    NAVIGATION_VIEWS.find((candidate) => candidate.id === activeView) ??
    NAVIGATION_VIEWS[0];
  const Panel = view.panel;

  return (
    <div className="flex h-full">
      <ActivityBar wide={wide} panelVisible={panelVisible} />
      {panelVisible && (
        <aside
          aria-label={view.label}
          className="w-64 shrink-0 border-r border-sidebar-border"
        >
          <Panel onNavigate={onNavigate} />
        </aside>
      )}
    </div>
  );
}
