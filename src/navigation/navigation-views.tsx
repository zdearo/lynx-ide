import type { ComponentType } from "react";
import { FolderGit2, type LucideIcon } from "lucide-react";

import type { NavigationViewId } from "@/navigation/navigation-store";
import { copy } from "@/ui-copy";
import { ProjectsPanel } from "@/workspace/ProjectsPanel";

export interface NavigationView {
  id: NavigationViewId;
  label: string;
  icon: LucideIcon;
  /** What the side panel shows while this view is active. */
  panel: ComponentType<{ onNavigate?: () => void }>;
}

/** Add a view here and it appears in the primary bar. */
export const NAVIGATION_VIEWS: readonly NavigationView[] = [
  {
    id: "projects",
    label: copy.navigation.projects,
    icon: FolderGit2,
    panel: ProjectsPanel,
  },
];
