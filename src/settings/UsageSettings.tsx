import type { UsageConfig } from "@/bindings";
import {
  ChoiceSetting,
  StepperSetting,
  SwitchSetting,
} from "@/settings/SettingFields";
import {
  criticalAtBounds,
  providersWith,
  warnAtBounds,
} from "@/settings/usage-settings";
import type { ChoiceOption } from "@/settings/choice-field";
import { useSettings } from "@/settings/use-settings";
import { DEFAULT_USAGE_CONFIG } from "@/config/config-store";
import { copy } from "@/ui-copy";

const text = copy.settings.usage;

const DISPLAY_OPTIONS: ChoiceOption[] = [
  { value: "used", label: text.display.used },
  { value: "remaining", label: text.display.remaining },
];

const LAYOUT_OPTIONS: ChoiceOption[] = [
  { value: "compact", label: text.layout.compact },
  { value: "detailed", label: text.layout.detailed },
];

function isDisplay(value: string | null): value is UsageConfig["display"] {
  return value === "used" || value === "remaining";
}

function isLayout(value: string | null): value is UsageConfig["layout"] {
  return value === "compact" || value === "detailed";
}

export function UsageSettings() {
  const { config, disabled, save } = useSettings();
  if (!config) return null;
  const usage = config.usage;
  const update = (patch: Partial<UsageConfig>) =>
    save({ ...config, usage: { ...usage, ...patch } });
  const warnBounds = warnAtBounds(usage.criticalAt);
  const criticalBounds = criticalAtBounds(usage.warnAt);

  return (
    <>
      <p className="text-nested text-muted-foreground">{text.intro}</p>
      <SwitchSetting
        id="usage-visible"
        label={text.visible.label}
        hint={text.visible.hint}
        checked={usage.visible}
        disabled={disabled}
        onCommit={(visible) => update({ visible })}
      />
      <div className="grid gap-0.5">
        <p className="text-ui font-medium">{text.providers.label}</p>
        <p className="text-nested text-muted-foreground">
          {text.providers.hint}
        </p>
      </div>
      <SwitchSetting
        id="usage-provider-claude"
        label={text.providers.claude}
        hint={text.providers.claudeHint}
        checked={usage.providers.includes("claude")}
        disabled={disabled}
        onCommit={(on) =>
          update({ providers: providersWith(usage.providers, "claude", on) })
        }
      />
      <SwitchSetting
        id="usage-provider-codex"
        label={text.providers.codex}
        hint={text.providers.codexHint}
        checked={usage.providers.includes("codex")}
        disabled={disabled}
        onCommit={(on) =>
          update({ providers: providersWith(usage.providers, "codex", on) })
        }
      />
      <SwitchSetting
        id="usage-show-names"
        label={text.showNames.label}
        hint={text.showNames.hint}
        checked={usage.showNames}
        disabled={disabled}
        onCommit={(showNames) => update({ showNames })}
      />
      <ChoiceSetting
        id="usage-display"
        label={text.display.label}
        hint={text.display.hint}
        value={usage.display}
        defaultValue={DEFAULT_USAGE_CONFIG.display}
        options={DISPLAY_OPTIONS}
        disabled={disabled}
        onCommit={async (display) => {
          if (isDisplay(display)) await update({ display });
        }}
      />
      <ChoiceSetting
        id="usage-layout"
        label={text.layout.label}
        hint={text.layout.hint}
        value={usage.layout}
        defaultValue={DEFAULT_USAGE_CONFIG.layout}
        options={LAYOUT_OPTIONS}
        disabled={disabled}
        onCommit={async (layout) => {
          if (isLayout(layout)) await update({ layout });
        }}
      />
      <StepperSetting
        id="usage-warn-at"
        label={text.warnAt.label}
        hint={text.warnAt.hint}
        value={usage.warnAt}
        defaultValue={DEFAULT_USAGE_CONFIG.warnAt}
        min={warnBounds.min}
        max={warnBounds.max}
        disabled={disabled}
        onCommit={(warnAt) => update({ warnAt })}
      />
      <StepperSetting
        id="usage-critical-at"
        label={text.criticalAt.label}
        hint={text.criticalAt.hint}
        value={usage.criticalAt}
        defaultValue={DEFAULT_USAGE_CONFIG.criticalAt}
        min={criticalBounds.min}
        max={criticalBounds.max}
        disabled={disabled}
        onCommit={(criticalAt) => update({ criticalAt })}
      />
      <p className="text-meta text-muted-foreground" aria-live="polite">
        {text.levels(usage.warnAt, usage.criticalAt)}
      </p>
    </>
  );
}
