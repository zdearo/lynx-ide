import type { ComponentType } from "react";

import { FolderGlyph18 } from "@/components/icons/FolderGlyph18";
import type { IconComponent } from "@/components/icons/icon-component";
import { TerminalGlyph18 } from "@/components/icons/TerminalGlyph18";
import { TerminalSettings } from "@/settings/TerminalSettings";
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
    id: "worktrees",
    label: copy.settings.worktrees.title,
    icon: FolderGlyph18,
    panel: WorktreesSettings,
  },
];
