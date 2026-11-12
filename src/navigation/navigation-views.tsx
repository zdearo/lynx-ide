import type { ComponentType } from "react";
import { FolderSimpleIcon, RobotIcon } from "@phosphor-icons/react";
import type { IconComponent } from "@/components/icons/icon-component";

import { BotsPanel } from "@/bots/BotsPanel";
import type { NavigationViewId } from "@/navigation/navigation-store";
import { copy } from "@/ui-copy";
import { ProjectsPanel } from "@/workspace/projects/ProjectsPanel";

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
    icon: FolderSimpleIcon,
    panel: ProjectsPanel,
  },
  {
    id: "bots",
    label: copy.navigation.bots,
    icon: RobotIcon,
    panel: BotsPanel,
  },
];
