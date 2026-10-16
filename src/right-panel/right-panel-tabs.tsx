import type { ComponentType } from "react";
import { FolderTreeGlyph18 } from "@/components/icons/FolderTreeGlyph18";
import type { IconComponent } from "@/components/icons/icon-component";

import { FilesPanel } from "@/files/FilesPanel";
import type { RightPanelTabId } from "@/right-panel/right-panel-store";
import { copy } from "@/ui-copy";

export interface RightPanelTab {
  id: RightPanelTabId;
  label: string;
  icon: IconComponent;
  panel: ComponentType;
}

/** Add a tab here and it appears in the side panel's tab bar. */
export const RIGHT_PANEL_TABS: readonly RightPanelTab[] = [
  {
    id: "files",
    label: copy.files.title,
    icon: FolderTreeGlyph18,
    panel: FilesPanel,
  },
];
