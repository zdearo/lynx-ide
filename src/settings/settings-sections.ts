import type { ComponentType } from "react";

import { TerminalSettings } from "@/settings/TerminalSettings";
import { WorktreesSettings } from "@/settings/WorktreesSettings";
import { copy } from "@/ui-copy";

export interface SettingsSection {
  id: string;
  label: string;
  panel: ComponentType;
}

/** Add a section here and it appears in the settings navigation. */
export const SETTINGS_SECTIONS: readonly SettingsSection[] = [
  {
    id: "terminal",
    label: copy.settings.terminal.title,
    panel: TerminalSettings,
  },
  {
    id: "worktrees",
    label: copy.settings.worktrees.title,
    panel: WorktreesSettings,
  },
];
