import { ActivityBar } from "@/navigation/ActivityBar";
import { useNavigationStore } from "@/navigation/navigation-store";
import { NAVIGATION_VIEWS } from "@/navigation/navigation-views";

/** The primary bar, which runs the full height of the window. */
export function NavigationRail({
  wide,
  panelVisible,
}: {
  wide: boolean;
  panelVisible: boolean;
}) {
  return <ActivityBar wide={wide} panelVisible={panelVisible} />;
}

/** The side panel of whichever view the rail has selected. The status bar sits below it. */
export function NavigationPanel({ onNavigate }: { onNavigate?: () => void }) {
  const activeView = useNavigationStore((state) => state.activeView);
  const view =
    NAVIGATION_VIEWS.find((candidate) => candidate.id === activeView) ??
    NAVIGATION_VIEWS[0];
  const Panel = view.panel;

  return (
    <aside
      aria-label={view.label}
      className="w-64 shrink-0 border-r border-sidebar-border"
    >
      <Panel onNavigate={onNavigate} />
    </aside>
  );
}

interface NavigationSidebarProps {
  wide: boolean;
  panelVisible: boolean;
  /** Called after a choice in the panel, so a drawer can close itself. */
  onNavigate?: () => void;
}

/** The rail and the panel together, as the drawer shows them on a narrow window. */
export function NavigationSidebar({
  wide,
  panelVisible,
  onNavigate,
}: NavigationSidebarProps) {
  return (
    <div className="flex h-full">
      <NavigationRail wide={wide} panelVisible={panelVisible} />
      {panelVisible && <NavigationPanel onNavigate={onNavigate} />}
    </div>
  );
}
