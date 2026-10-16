import type { HarnessId } from "@/bindings";

/** What a new terminal can start. `command` is typed into the shell; null opens a plain shell. */
export interface TerminalPreset {
  id: string;
  label: string;
  command: string | null;
  kind: "shell" | "agent";
  /** The agent this preset starts; null for a plain shell. */
  harness: HarnessId | null;
}

export const TERMINAL_PRESETS: readonly TerminalPreset[] = [
  {
    id: "shell",
    label: "Shell",
    command: null,
    kind: "shell",
    harness: null,
  },
  {
    id: "claude",
    label: "Claude Code",
    command: "claude",
    kind: "agent",
    harness: "claude",
  },
  {
    id: "codex",
    label: "Codex",
    command: "codex",
    kind: "agent",
    harness: "codex",
  },
  {
    id: "opencode",
    label: "OpenCode",
    command: "opencode",
    kind: "agent",
    harness: "opencode",
  },
  {
    id: "antigravity",
    label: "Antigravity",
    command: "agy",
    kind: "agent",
    harness: "antigravity",
  },
  {
    id: "cursor",
    label: "Cursor",
    command: "cursor-agent",
    kind: "agent",
    harness: "cursor",
  },
  {
    id: "copilot",
    label: "GitHub Copilot",
    command: "copilot",
    kind: "agent",
    harness: "copilot",
  },
  {
    id: "grok",
    label: "Grok",
    command: "grok",
    kind: "agent",
    harness: "grok",
  },
  {
    id: "droid",
    label: "Droid",
    command: "droid",
    kind: "agent",
    harness: "droid",
  },
  {
    id: "kiro",
    label: "Kiro",
    command: "kiro-cli",
    kind: "agent",
    harness: "kiro",
  },
  {
    id: "qwen",
    label: "Qwen Code",
    command: "qwen",
    kind: "agent",
    harness: "qwen",
  },
  {
    id: "pi",
    label: "Pi",
    command: "pi",
    kind: "agent",
    harness: "pi",
  },
  {
    id: "omp",
    label: "OMP",
    command: "omp",
    kind: "agent",
    harness: "omp",
  },
];

/** The presets a new terminal offers: shells always, an agent only while it is on in Settings. */
export function visiblePresets(
  on: ReadonlySet<HarnessId>,
): readonly TerminalPreset[] {
  return TERMINAL_PRESETS.filter(
    (preset) => preset.harness === null || on.has(preset.harness),
  );
}
