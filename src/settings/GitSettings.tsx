import type { CommitMessageAgent, CommitMessageStyle } from "@/bindings";
import type { ChoiceOption } from "@/settings/choice-field";
import { ChoiceSetting, TextAreaSetting } from "@/settings/SettingFields";
import { useSettings } from "@/settings/use-settings";
import { copy } from "@/ui-copy";

const AGENT_OPTIONS: ChoiceOption[] = [
  { value: "auto", label: copy.settings.git.agent.auto },
  { value: "claude", label: copy.settings.git.agent.claude },
  { value: "codex", label: copy.settings.git.agent.codex },
];

const STYLE_OPTIONS: ChoiceOption[] = [
  { value: "conventional", label: copy.settings.git.style.conventional },
  { value: "plain", label: copy.settings.git.style.plain },
  { value: "custom", label: copy.settings.git.style.custom },
];

function isAgent(value: string | null): value is CommitMessageAgent {
  return value === "auto" || value === "claude" || value === "codex";
}

function isStyle(value: string | null): value is CommitMessageStyle {
  return value === "conventional" || value === "plain" || value === "custom";
}

export function GitSettings() {
  const { config, disabled, save } = useSettings();
  if (!config) return null;
  const git = config.git;

  return (
    <>
      <ChoiceSetting
        id="git-commit-agent"
        label={copy.settings.git.agent.label}
        hint={copy.settings.git.agent.hint}
        value={git.commitMessageAgent}
        defaultValue="auto"
        options={AGENT_OPTIONS}
        disabled={disabled}
        onCommit={async (agent) => {
          if (isAgent(agent)) {
            await save({
              ...config,
              git: { ...git, commitMessageAgent: agent },
            });
          }
        }}
      />
      <ChoiceSetting
        id="git-commit-style"
        label={copy.settings.git.style.label}
        hint={copy.settings.git.style.hint}
        value={git.commitMessageStyle}
        defaultValue="conventional"
        options={STYLE_OPTIONS}
        disabled={disabled}
        onCommit={async (style) => {
          if (isStyle(style)) {
            await save({
              ...config,
              git: { ...git, commitMessageStyle: style },
            });
          }
        }}
      />
      {git.commitMessageStyle === "custom" && (
        <TextAreaSetting
          id="git-commit-instructions"
          label={copy.settings.git.instructions.label}
          hint={copy.settings.git.instructions.hint}
          placeholder={copy.settings.git.instructions.placeholder}
          value={git.commitMessageInstructions}
          disabled={disabled}
          onCommit={(instructions) =>
            save({
              ...config,
              git: { ...git, commitMessageInstructions: instructions },
            })
          }
        />
      )}
    </>
  );
}
