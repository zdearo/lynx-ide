import type { ComponentType } from "react";

import { BellGlyph18 } from "@/components/icons/BellGlyph18";
import { CodeBranchGlyph18 } from "@/components/icons/CodeBranchGlyph18";
import { GlobeGlyph18 } from "@/components/icons/GlobeGlyph18";
import { FolderGlyph18 } from "@/components/icons/FolderGlyph18";
import type { IconComponent } from "@/components/icons/icon-component";
import { SparkleGlyph18 } from "@/components/icons/SparkleGlyph18";
import { RefreshGlyph18 } from "@/components/icons/RefreshGlyph18";
import { TerminalGlyph18 } from "@/components/icons/TerminalGlyph18";
import { AgentsSettings } from "@/settings/AgentsSettings";
import { BrowserSettings } from "@/settings/BrowserSettings";
import { GitSettings } from "@/settings/GitSettings";
import { NotificationsSettings } from "@/settings/NotificationsSettings";
import { TerminalSettings } from "@/settings/TerminalSettings";
import { UpdatesSettings } from "@/settings/UpdatesSettings";
import { WorktreesSettings } from "@/settings/WorktreesSettings";
import { copy } from "@/ui-copy";

export interface SettingsSection {
  id: string;
  label: string;
  icon: IconComponent;
  panel: ComponentType;
}

/** Add a section here and it appears in the settings navigation. */
export const SETTINGS_SECTIONS: readonly SettingsSection[] = [
  {
    id: "terminal",
    label: copy.settings.terminal.title,
    icon: TerminalGlyph18,
    panel: TerminalSettings,
  },
  {
    id: "agents",
    label: copy.settings.agents.title,
    icon: SparkleGlyph18,
    panel: AgentsSettings,
  },
  {
    id: "worktrees",
    label: copy.settings.worktrees.title,
    icon: FolderGlyph18,
    panel: WorktreesSettings,
  },
  {
    id: "git",
    label: copy.settings.git.title,
    icon: CodeBranchGlyph18,
    panel: GitSettings,
  },
  {
    id: "browser",
    label: copy.settings.browser.title,
    icon: GlobeGlyph18,
    panel: BrowserSettings,
  },
  {
    id: "notifications",
    label: copy.settings.notifications.title,
    icon: BellGlyph18,
    panel: NotificationsSettings,
  },
  {
    id: "updates",
    label: copy.settings.updates.title,
    icon: RefreshGlyph18,
    panel: UpdatesSettings,
  },
];
