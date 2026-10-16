/** What a new terminal can start. `command` is typed into the shell; null opens a plain shell. */
export interface TerminalPreset {
  id: string;
  label: string;
  command: string | null;
  kind: "shell" | "agent";
}

export const TERMINAL_PRESETS: readonly TerminalPreset[] = [
  { id: "shell", label: "Shell", command: null, kind: "shell" },
  { id: "claude", label: "Claude Code", command: "claude", kind: "agent" },
  { id: "codex", label: "Codex", command: "codex", kind: "agent" },
  { id: "opencode", label: "OpenCode", command: "opencode", kind: "agent" },
];

/** Whether a session was started from an agent preset (it gets the agent glyph). */
export function isAgentCommand(command: string | null): boolean {
  if (!command) return false;
  const program = command.trim().split(/\s+/)[0];
  return TERMINAL_PRESETS.some(
    (preset) => preset.kind === "agent" && preset.command === program,
  );
}
