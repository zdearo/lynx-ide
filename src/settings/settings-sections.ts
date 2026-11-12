import type { ComponentType } from "react";

import {
  ArrowsClockwiseIcon,
  BellIcon,
  FolderIcon,
  GitBranchIcon,
  GlobeIcon,
  LockIcon,
  SparkleIcon,
  TerminalIcon,
  TreeViewIcon,
} from "@phosphor-icons/react";
import type { IconComponent } from "@/components/icons/icon-component";
import { AgentsSettings } from "@/settings/AgentsSettings";
import { BotsSettings } from "@/settings/BotsSettings";
import { BrowserSettings } from "@/settings/BrowserSettings";
import { GitSettings } from "@/settings/GitSettings";
import { NotificationsSettings } from "@/settings/NotificationsSettings";
import { OrchestrationSettings } from "@/settings/OrchestrationSettings";
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
    icon: TerminalIcon,
    panel: TerminalSettings,
  },
  {
    id: "agents",
    label: copy.settings.agents.title,
    icon: SparkleIcon,
    panel: AgentsSettings,
  },
  {
    id: "bots",
    label: copy.settings.bots.title,
    icon: LockIcon,
    panel: BotsSettings,
  },
  {
    id: "worktrees",
    label: copy.settings.worktrees.title,
    icon: FolderIcon,
    panel: WorktreesSettings,
  },
  {
    id: "git",
    label: copy.settings.git.title,
    icon: GitBranchIcon,
    panel: GitSettings,
  },
  {
    id: "orchestration",
    label: copy.settings.orchestration.title,
    icon: TreeViewIcon,
    panel: OrchestrationSettings,
  },
  {
    id: "browser",
    label: copy.settings.browser.title,
    icon: GlobeIcon,
    panel: BrowserSettings,
  },
  {
    id: "notifications",
    label: copy.settings.notifications.title,
    icon: BellIcon,
    panel: NotificationsSettings,
  },
  {
    id: "updates",
    label: copy.settings.updates.title,
    icon: ArrowsClockwiseIcon,
    panel: UpdatesSettings,
  },
];
