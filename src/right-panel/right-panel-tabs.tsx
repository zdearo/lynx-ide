import type { ComponentType } from "react";
import { FolderTree, type LucideIcon } from "lucide-react";

import { FilesPanel } from "@/files/FilesPanel";
import type { RightPanelTabId } from "@/right-panel/right-panel-store";
import { copy } from "@/ui-copy";

export interface RightPanelTab {
  id: RightPanelTabId;
  label: string;
  icon: LucideIcon;
  panel: ComponentType;
}

/** Add a tab here and it appears in the side panel's tab bar. */
export const RIGHT_PANEL_TABS: readonly RightPanelTab[] = [
  {
    id: "files",
    label: copy.files.title,
    icon: FolderTree,
    panel: FilesPanel,
  },
];
