import type { AppConfig } from "@/bindings";
import { DEFAULT_TERMINAL_CONFIG } from "@/config/config-store";
import { NumberSetting, TextSetting } from "@/settings/SettingFields";
import { useSettings } from "@/settings/use-settings";
import { copy } from "@/ui-copy";

export function TerminalSettings() {
  const { config, disabled, save } = useSettings();
  if (!config) return null;
  const terminal = config.terminal;
  const update = (patch: Partial<AppConfig["terminal"]>) =>
    save({ ...config, terminal: { ...terminal, ...patch } });

  return (
    <div className="grid gap-5">
      <TextSetting
        id="terminal-font-family"
        label={copy.settings.terminal.fontFamily.label}
        hint={copy.settings.terminal.fontFamily.hint}
        placeholder={copy.settings.terminal.fontFamily.placeholder}
        value={terminal.fontFamily}
        disabled={disabled}
        onCommit={(fontFamily) => update({ fontFamily })}
      />
      <NumberSetting
        id="terminal-font-size"
        label={copy.settings.terminal.fontSize.label}
        hint={copy.settings.terminal.fontSize.hint}
        value={terminal.fontSize}
        defaultValue={DEFAULT_TERMINAL_CONFIG.fontSize}
        disabled={disabled}
        onCommit={(fontSize) => update({ fontSize })}
      />
      <TextSetting
        id="terminal-shell"
        label={copy.settings.terminal.shell.label}
        hint={copy.settings.terminal.shell.hint}
        placeholder={copy.settings.terminal.shell.placeholder}
        value={terminal.shell}
        disabled={disabled}
        onCommit={(shell) => update({ shell })}
      />
      <NumberSetting
        id="terminal-scrollback"
        label={copy.settings.terminal.scrollback.label}
        hint={copy.settings.terminal.scrollback.hint}
        value={terminal.scrollback}
        defaultValue={DEFAULT_TERMINAL_CONFIG.scrollback}
        disabled={disabled}
        onCommit={(scrollback) => update({ scrollback })}
      />
    </div>
  );
}
