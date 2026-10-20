import { useEffect } from "react";

import type { AgentBrowserAccess } from "@/bindings";
import { EngineProgressBar } from "@/browser/EngineProgressBar";
import { useBrowserStore } from "@/browser/browser-store";
import { Button } from "@/components/ui/button";
import { Label } from "@/components/ui/label";
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

/** What engine is installed, and the buttons to download, update or remove it. */
function EngineSetting() {
  const engine = useBrowserStore((s) => s.engine);
  const busy = useBrowserStore((s) => s.engineBusy);
  const error = useBrowserStore((s) => s.engineError);
  const load = useBrowserStore((s) => s.loadEngine);
  const install = useBrowserStore((s) => s.installEngine);
  const remove = useBrowserStore((s) => s.removeEngine);
  const text = copy.settings.browser.engine;

  useEffect(() => {
    void load();
  }, [load]);

  if (!engine) return null;
  const status =
    engine.state === "missing"
      ? text.missing
      : engine.state === "outdated"
        ? text.outdated
        : text.ready(engine.version);

  return (
    <div className="grid gap-1.5">
      <Label id="browser-engine-label">{text.label}</Label>
      <div
        role="group"
        aria-labelledby="browser-engine-label"
        className="flex items-center justify-between gap-3"
      >
        <span className="min-w-0 text-sm" role="status">
          {status}
        </span>
        <div className="flex shrink-0 gap-2">
          <Button
            size="sm"
            variant="outline"
            disabled={busy}
            onClick={() => void install(engine.state !== "missing")}
          >
            {engine.state === "missing"
              ? text.install
              : engine.state === "outdated"
                ? text.update
                : text.reinstall}
          </Button>
          {engine.state !== "missing" && (
            <Button
              size="sm"
              variant="ghost"
              disabled={busy}
              onClick={() => void remove()}
            >
              {text.remove}
            </Button>
          )}
        </div>
      </div>
      {busy && <EngineProgressBar />}
      {error ? (
        <p role="alert" className="text-nested text-destructive">
          {error}
        </p>
      ) : (
        <p className="text-nested text-muted-foreground">{text.hint}</p>
      )}
    </div>
  );
}

export function BrowserSettings() {
  const { config, disabled, save } = useSettings();
  if (!config) return null;
  const browser = config.browser;

  return (
    <>
      <EngineSetting />
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
