import type { ComponentType } from "react";

import { BellGlyph18 } from "@/components/icons/BellGlyph18";
import { CodeBranchGlyph18 } from "@/components/icons/CodeBranchGlyph18";
import { FolderGlyph18 } from "@/components/icons/FolderGlyph18";
import type { IconComponent } from "@/components/icons/icon-component";
import { SparkleGlyph18 } from "@/components/icons/SparkleGlyph18";
import { TerminalGlyph18 } from "@/components/icons/TerminalGlyph18";
import { AgentsSettings } from "@/settings/AgentsSettings";
import { GitSettings } from "@/settings/GitSettings";
import { NotificationsSettings } from "@/settings/NotificationsSettings";
import { TerminalSettings } from "@/settings/TerminalSettings";
import { WorktreesSettings } from "@/settings/WorktreesSettings";
import { copy } from "@/ui-copy";

export interface SettingsSection {
  id: string;
  label: string;
  icon: IconComponent;
  panel: ComponentType;
  /** The page uses the whole width of the dialog instead of a reading column. */
  fillWidth?: boolean;
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
    fillWidth: true,
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
    id: "notifications",
    label: copy.settings.notifications.title,
    icon: BellGlyph18,
    panel: NotificationsSettings,
  },
];
