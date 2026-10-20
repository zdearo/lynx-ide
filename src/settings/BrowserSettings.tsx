import type { AgentBrowserAccess } from "@/bindings";
import type { ChoiceOption } from "@/settings/choice-field";
import { ChoiceSetting, TextAreaSetting } from "@/settings/SettingFields";
import { useSettings } from "@/settings/use-settings";
import { copy } from "@/ui-copy";

const ACCESS_OPTIONS: ChoiceOption[] = [
  { value: "full", label: copy.settings.browser.access.full },
  { value: "readOnly", label: copy.settings.browser.access.readOnly },
  { value: "off", label: copy.settings.browser.access.off },
];

function isAccess(value: string | null): value is AgentBrowserAccess {
  return value === "full" || value === "readOnly" || value === "off";
}

export function BrowserSettings() {
  const { config, disabled, save } = useSettings();
  if (!config) return null;
  const browser = config.browser;

  return (
    <>
      <ChoiceSetting
        id="browser-agent-access"
        label={copy.settings.browser.access.label}
        hint={copy.settings.browser.access.hint}
        value={browser.agentAccess}
        defaultValue="full"
        options={ACCESS_OPTIONS}
        disabled={disabled}
        onCommit={async (access) => {
          if (isAccess(access)) {
            await save({
              ...config,
              browser: { ...browser, agentAccess: access },
            });
          }
        }}
      />
      <TextAreaSetting
        id="browser-allowed-sites"
        label={copy.settings.browser.sites.label}
        hint={copy.settings.browser.sites.hint}
        placeholder={copy.settings.browser.sites.placeholder}
        value={
          browser.allowedSites.length > 0
            ? browser.allowedSites.join("\n")
            : null
        }
        disabled={disabled}
        onCommit={(text) =>
          save({
            ...config,
            browser: {
              ...browser,
              allowedSites: (text ?? "")
                .split("\n")
                .map((site) => site.trim())
                .filter((site) => site !== ""),
            },
          })
        }
      />
    </>
  );
}
