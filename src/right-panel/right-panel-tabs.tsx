import type { ComponentType } from "react";
import { BrowserTabPanel } from "@/browser/BrowserTabPanel";
import { ChangesPanel } from "@/changes/ChangesPanel";
import { CodeCompareGlyph18 } from "@/components/icons/CodeCompareGlyph18";
import { CodePullRequestGlyph18 } from "@/components/icons/CodePullRequestGlyph18";
import { FolderTreeGlyph18 } from "@/components/icons/FolderTreeGlyph18";
import { GlobeGlyph18 } from "@/components/icons/GlobeGlyph18";
import type { IconComponent } from "@/components/icons/icon-component";

import { FilesPanel } from "@/files/FilesPanel";
import { PullRequestPanel } from "@/pull-request/PullRequestPanel";
import type { RightPanelTabId } from "@/right-panel/right-panel-store";
import { copy } from "@/ui-copy";

export interface RightPanelTab {
  id: RightPanelTabId;
  label: string;
  icon: IconComponent;
  panel: ComponentType;
  /** Draws a thin divider before this tab, to set off a group of tabs. */
  separatorBefore?: boolean;
}

/** Add a tab here and it appears in the side panel's tab bar. */
export const RIGHT_PANEL_TABS: readonly RightPanelTab[] = [
  {
    id: "files",
    label: copy.files.title,
    icon: FolderTreeGlyph18,
    panel: FilesPanel,
  },
  {
    id: "changes",
    label: copy.changes.title,
    icon: CodeCompareGlyph18,
    panel: ChangesPanel,
  },
  {
    id: "pull-request",
    label: copy.pullRequest.tab,
    icon: CodePullRequestGlyph18,
    panel: PullRequestPanel,
  },
  {
    id: "browser",
    label: copy.browser.tab,
    icon: GlobeGlyph18,
    panel: BrowserTabPanel,
    separatorBefore: true,
  },
];
