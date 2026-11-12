import type { AppConfig, ColorScheme } from "@/bindings";
import { PaletteIcon, SlidersIcon, TextAaIcon } from "@phosphor-icons/react";
import { Tabs, TabsContent, TabsList, TabsTrigger } from "@/components/ui/tabs";
import { DEFAULT_TERMINAL_CONFIG } from "@/config/config-store";
import { isMacPlatform } from "@/platform-shortcuts";
import type { ChoiceOption } from "@/settings/choice-field";
import {
  ChoiceSetting,
  StepperSetting,
  SwitchSetting,
} from "@/settings/SettingFields";
import { PalettePreview } from "@/settings/PalettePreview";
import { useInstalledShells } from "@/settings/use-installed-shells";
import { useSettings } from "@/settings/use-settings";
import {
  isColourScheme,
  resolveTerminalTheme,
} from "@/terminal/terminal-color-schemes";
import { copy } from "@/ui-copy";

const MIN_FONT_SIZE = 8;
const MAX_FONT_SIZE = 32;

// Common monospace families per platform. A family not installed here falls back to the bundled font.
const FONT_PRESETS: ChoiceOption[] = [
  { value: null, label: copy.settings.terminal.fontFamily.bundled },
  {
    value: "SF Mono, Menlo, monospace",
    label: copy.settings.terminal.fontFamily.sfMono,
  },
  {
    value: "Cascadia Mono, Consolas, monospace",
    label: copy.settings.terminal.fontFamily.cascadia,
  },
  {
    value: "Fira Code, monospace",
    label: copy.settings.terminal.fontFamily.fira,
  },
];

const numbers = new Intl.NumberFormat();
const SCROLLBACK_OPTIONS: ChoiceOption[] = [
  1_000, 5_000, 10_000, 50_000, 100_000,
].map((lines) => ({ value: String(lines), label: numbers.format(lines) }));

const BELL_OPTIONS: ChoiceOption[] = [
  { value: "none", label: copy.settings.terminal.bell.none },
  { value: "visual", label: copy.settings.terminal.bell.visual },
];

// Every theme's name comes from the copy, typed so a theme cannot be left out of it.
const THEME_NAMES: Record<ColorScheme, string> = copy.settings.terminal.themes;
const THEMES: ChoiceOption[] = Object.entries(THEME_NAMES).map(
  ([value, label]) => ({ value, label }),
);

export function TerminalSettings() {
  const { config, disabled, save } = useSettings();
  const shells = useInstalledShells();
  if (!config) return null;
  const terminal = config.terminal;
  const update = (patch: Partial<AppConfig["terminal"]>) =>
    save({ ...config, terminal: { ...terminal, ...patch } });
  const shellOptions: ChoiceOption[] = [
    { value: null, label: copy.settings.terminal.shell.login },
    ...shells.map((path) => ({ value: path, label: path })),
  ];
  const theme = resolveTerminalTheme(terminal);

  return (
    <Tabs defaultValue="text" className="gap-6">
      <TabsList variant="line" aria-label={copy.settings.terminal.tabs.label}>
        <TabsTrigger value="text">
          <TextAaIcon aria-hidden className="size-3.5" />
          {copy.settings.terminal.tabs.text}
        </TabsTrigger>
        <TabsTrigger value="appearance">
          <PaletteIcon aria-hidden className="size-3.5" />
          {copy.settings.terminal.tabs.appearance}
        </TabsTrigger>
        <TabsTrigger value="behavior">
          <SlidersIcon aria-hidden className="size-3.5" />
          {copy.settings.terminal.tabs.behavior}
        </TabsTrigger>
      </TabsList>

      <TabsContent value="text" className="grid gap-5">
        <ChoiceSetting
          id="terminal-font-family"
          label={copy.settings.terminal.fontFamily.label}
          hint={copy.settings.terminal.fontFamily.hint}
          value={terminal.fontFamily}
          defaultValue={DEFAULT_TERMINAL_CONFIG.fontFamily}
          options={FONT_PRESETS}
          custom={{
            label: copy.settings.terminal.fontFamily.custom,
            placeholder: copy.settings.terminal.fontFamily.placeholder,
          }}
          disabled={disabled}
          onCommit={(fontFamily) => update({ fontFamily })}
        />
        <StepperSetting
          id="terminal-font-size"
          label={copy.settings.terminal.fontSize.label}
          hint={copy.settings.terminal.fontSize.hint}
          value={terminal.fontSize}
          defaultValue={DEFAULT_TERMINAL_CONFIG.fontSize}
          min={MIN_FONT_SIZE}
          max={MAX_FONT_SIZE}
          disabled={disabled}
          onCommit={(fontSize) => update({ fontSize })}
        />
        <ChoiceSetting
          id="terminal-scrollback"
          label={copy.settings.terminal.scrollback.label}
          hint={copy.settings.terminal.scrollback.hint}
          value={String(terminal.scrollback)}
          defaultValue={String(DEFAULT_TERMINAL_CONFIG.scrollback)}
          options={SCROLLBACK_OPTIONS}
          disabled={disabled}
          onCommit={async (lines) => {
            if (lines) await update({ scrollback: Number(lines) });
          }}
        />
      </TabsContent>

      <TabsContent value="appearance" className="grid gap-5">
        <ChoiceSetting
          id="terminal-color-scheme"
          label={copy.settings.terminal.colorScheme.label}
          hint={copy.settings.terminal.colorScheme.hint}
          value={terminal.colorScheme}
          defaultValue={DEFAULT_TERMINAL_CONFIG.colorScheme}
          options={THEMES}
          disabled={disabled}
          onCommit={async (value) => {
            if (value !== null && isColourScheme(value)) {
              await update({ colorScheme: value });
            }
          }}
        />
        <ChoiceSetting
          id="terminal-text-colours"
          label={copy.settings.terminal.textColours.label}
          hint={copy.settings.terminal.textColours.hint}
          value={terminal.textColorScheme}
          defaultValue={DEFAULT_TERMINAL_CONFIG.textColorScheme}
          options={[
            {
              value: null,
              label: copy.settings.terminal.textColours.sameAsTheme,
            },
            ...THEMES,
          ]}
          disabled={disabled}
          onCommit={(value) =>
            update({
              textColorScheme:
                value !== null && isColourScheme(value) ? value : null,
            })
          }
        />
        <PalettePreview theme={theme} />
      </TabsContent>

      <TabsContent value="behavior" className="grid gap-5">
        <ChoiceSetting
          id="terminal-shell"
          label={copy.settings.terminal.shell.label}
          hint={copy.settings.terminal.shell.hint}
          value={terminal.shell}
          defaultValue={DEFAULT_TERMINAL_CONFIG.shell}
          options={shellOptions}
          custom={{
            label: copy.settings.terminal.shell.custom,
            placeholder: copy.settings.terminal.shell.placeholder,
          }}
          disabled={disabled}
          onCommit={(shell) => update({ shell })}
        />
        <SwitchSetting
          id="terminal-copy-on-select"
          label={copy.settings.terminal.copyOnSelect.label}
          hint={copy.settings.terminal.copyOnSelect.hint}
          checked={terminal.copyOnSelect}
          disabled={disabled}
          onCommit={(copyOnSelect) => update({ copyOnSelect })}
        />
        <SwitchSetting
          id="terminal-pty-daemon"
          label={copy.settings.terminal.ptyDaemon.label}
          hint={copy.settings.terminal.ptyDaemon.hint}
          checked={terminal.ptyDaemon}
          disabled={disabled}
          onCommit={(ptyDaemon) => update({ ptyDaemon })}
        />
        <ChoiceSetting
          id="terminal-bell"
          label={copy.settings.terminal.bell.label}
          hint={copy.settings.terminal.bell.hint}
          value={terminal.bell}
          defaultValue={DEFAULT_TERMINAL_CONFIG.bell}
          options={BELL_OPTIONS}
          disabled={disabled}
          onCommit={(bell) =>
            update({ bell: bell === "none" ? "none" : "visual" })
          }
        />
        {isMacPlatform() && (
          <SwitchSetting
            id="terminal-option-as-meta"
            label={copy.settings.terminal.optionAsMeta.label}
            hint={copy.settings.terminal.optionAsMeta.hint}
            checked={terminal.macOptionIsMeta}
            disabled={disabled}
            onCommit={(macOptionIsMeta) => update({ macOptionIsMeta })}
          />
        )}
        <SwitchSetting
          id="terminal-close-on-exit"
          label={copy.settings.terminal.closeOnExit.label}
          hint={copy.settings.terminal.closeOnExit.hint}
          checked={terminal.closeOnExit}
          disabled={disabled}
          onCommit={(closeOnExit) => update({ closeOnExit })}
        />
        <SwitchSetting
          id="terminal-confirm-close"
          label={copy.settings.terminal.confirmClose.label}
          hint={copy.settings.terminal.confirmClose.hint}
          checked={terminal.confirmClose}
          disabled={disabled}
          onCommit={(confirmClose) => update({ confirmClose })}
        />
      </TabsContent>
    </Tabs>
  );
}
