import { TextSetting } from "@/settings/SettingFields";
import { useSettings } from "@/settings/use-settings";
import { copy } from "@/ui-copy";

export function WorktreesSettings() {
  const { config, disabled, save } = useSettings();
  if (!config) return null;

  return (
    <TextSetting
      id="worktrees-directory"
      label={copy.settings.worktrees.directory.label}
      hint={copy.settings.worktrees.directory.hint}
      placeholder={copy.settings.worktrees.directory.placeholder}
      value={config.worktrees.directory}
      disabled={disabled}
      onCommit={(directory) =>
        save({ ...config, worktrees: { ...config.worktrees, directory } })
      }
    />
  );
}
