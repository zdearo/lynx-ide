import { GearGlyph18 } from "@/components/icons/GearGlyph18";

import { TooltipIconButton } from "@/components/TooltipIconButton";
import { cn } from "@/lib/utils";
import { NAVIGATION_VIEWS } from "@/navigation/navigation-views";
import { useNavigationStore } from "@/navigation/navigation-store";
import { shortcutLabel } from "@/platform-shortcuts";
import { copy } from "@/ui-copy";

interface ActivityBarProps {
  wide: boolean;
  /** Whether the side panel is showing, so the active view can say it is pressed. */
  panelVisible: boolean;
}

/** The primary bar: one button per view at the top, settings at the bottom. */
export function ActivityBar({ wide, panelVisible }: ActivityBarProps) {
  const activeView = useNavigationStore((state) => state.activeView);
  const selectView = useNavigationStore((state) => state.selectView);
  const openSettings = useNavigationStore((state) => state.openSettings);

  return (
    <nav
      aria-label={copy.navigation.label}
      className="flex w-11 shrink-0 flex-col items-center justify-between border-r border-sidebar-border bg-background py-1.5"
    >
      <ul className="flex flex-col gap-1">
        {NAVIGATION_VIEWS.map((view) => {
          const pressed = view.id === activeView && panelVisible;
          return (
            <li key={view.id}>
              <TooltipIconButton
                label={view.label}
                tooltipSide="right"
                aria-pressed={pressed}
                onClick={() => selectView(view.id, wide)}
                className={cn(
                  "size-9",
                  pressed
                    ? "bg-sidebar-accent text-foreground"
                    : "text-muted-foreground hover:text-foreground",
                )}
              >
                <view.icon className="size-5" />
              </TooltipIconButton>
            </li>
          );
        })}
      </ul>
      <TooltipIconButton
        label={copy.navigation.settings}
        shortcut={shortcutLabel("openSettings")}
        tooltipSide="right"
        onClick={() => openSettings()}
        className="size-9 text-muted-foreground hover:text-foreground"
      >
        <GearGlyph18 className="size-5" />
      </TooltipIconButton>
    </nav>
  );
}
