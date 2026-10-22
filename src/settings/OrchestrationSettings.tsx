import { SwitchSetting } from "@/settings/SettingFields";
import { useSettings } from "@/settings/use-settings";
import { copy } from "@/ui-copy";

export function OrchestrationSettings() {
  const { config, disabled, save } = useSettings();
  if (!config) return null;
  const orchestration = config.orchestration;

  return (
    <>
      <p className="text-nested text-muted-foreground">
        {copy.settings.orchestration.intro}
      </p>
      <SwitchSetting
        id="orchestration-enabled"
        label={copy.settings.orchestration.enabled.label}
        hint={copy.settings.orchestration.enabled.hint}
        checked={orchestration.enabled}
        disabled={disabled}
        onCommit={(enabled) =>
          save({ ...config, orchestration: { ...orchestration, enabled } })
        }
      />
      <SwitchSetting
        id="orchestration-release"
        label={copy.settings.orchestration.release.label}
        hint={copy.settings.orchestration.release.hint}
        checked={orchestration.releaseFinishedWorkers}
        disabled={disabled}
        onCommit={(releaseFinishedWorkers) =>
          save({
            ...config,
            orchestration: { ...orchestration, releaseFinishedWorkers },
          })
        }
      />
    </>
  );
}
