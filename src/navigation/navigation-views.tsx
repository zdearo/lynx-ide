import type { ComponentType } from "react";
import { Folder2Glyph18 } from "@/components/icons/Folder2Glyph18";
import type { IconComponent } from "@/components/icons/icon-component";

import type { NavigationViewId } from "@/navigation/navigation-store";
import { copy } from "@/ui-copy";
import { ProjectsPanel } from "@/workspace/ProjectsPanel";

export interface NavigationView {
  id: NavigationViewId;
  label: string;
  icon: IconComponent;
  /** What the side panel shows while this view is active. */
  panel: ComponentType<{ onNavigate?: () => void }>;
}

/** Add a view here and it appears in the primary bar. */
export const NAVIGATION_VIEWS: readonly NavigationView[] = [
  {
    id: "projects",
    label: copy.navigation.projects,
    icon: Folder2Glyph18,
    panel: ProjectsPanel,
  },
];
