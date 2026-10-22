import type { AppConfig, HarnessId, LaunchMode } from "@/bindings";

/**
 * What a new terminal can start. `command` is typed into the shell; null opens a plain shell.
 * `yolo` is the flag that starts the agent without its permission prompts, or null when no such
 * flag is known for it.
 */
export interface TerminalPreset {
  id: string;
  label: string;
  command: string | null;
  kind: "shell" | "agent";
  /** The agent this preset starts; null for a plain shell. */
  harness: HarnessId | null;
  yolo: string | null;
}

// The YOLO flags were read from each CLI's --help where it is installed here (Claude Code, Codex,
// OpenCode and OMP).
export const TERMINAL_PRESETS: readonly TerminalPreset[] = [
  {
    id: "shell",
    label: "Shell",
    command: null,
    kind: "shell",
    harness: null,
    yolo: null,
  },
  {
    id: "claude",
    label: "Claude Code",
    command: "claude",
    kind: "agent",
    harness: "claude",
    yolo: "--dangerously-skip-permissions",
  },
  {
    id: "codex",
    label: "Codex",
    command: "codex",
    kind: "agent",
    harness: "codex",
    yolo: "--dangerously-bypass-approvals-and-sandbox",
  },
  {
    id: "opencode",
    label: "OpenCode",
    command: "opencode",
    kind: "agent",
    harness: "opencode",
    yolo: "--auto",
  },
  {
    id: "omp",
    label: "OMP",
    command: "omp",
    kind: "agent",
    harness: "omp",
    yolo: "--auto-approve",
  },
];

/** The arguments Settings sets for each agent. An agent is in the map when its arguments are on. */
export type LaunchArguments = Partial<Record<HarnessId, string>>;

/**
 * The line typed into the shell for a preset: its command, then in YOLO mode the agent's flag, then
 * the arguments set for it. A plain shell has no command, so it has no line.
 */
export function launchCommand(
  preset: TerminalPreset,
  launchArguments: LaunchArguments,
  mode: LaunchMode,
): string | null {
  if (preset.command === null) return null;
  const parts = [preset.command];
  if (preset.harness !== null) {
    if (mode === "yolo" && preset.yolo) parts.push(preset.yolo);
    const extra = launchArguments[preset.harness]?.trim();
    if (extra) parts.push(extra);
  }
  return parts.join(" ");
}

/** The flag an agent's YOLO mode adds, or null when none is known for it. */
export function yoloFlag(harness: HarnessId): string | null {
  return (
    TERMINAL_PRESETS.find((preset) => preset.harness === harness)?.yolo ?? null
  );
}

/** The presets a new terminal offers: shells always, an agent only while it is on in Settings. */
export function visiblePresets(
  on: ReadonlySet<HarnessId>,
): readonly TerminalPreset[] {
  return TERMINAL_PRESETS.filter(
    (preset) => preset.harness === null || on.has(preset.harness),
  );
}

/** The config with one agent's arguments turned on with this text, or off when the text is null. */
export function withLaunchArgument(
  config: AppConfig,
  harness: HarnessId,
  text: string | null,
): AppConfig {
  const launchArguments: LaunchArguments = {
    ...config.agents.launchArguments,
  };
  if (text === null) delete launchArguments[harness];
  else launchArguments[harness] = text;
  return { ...config, agents: { ...config.agents, launchArguments } };
}

/** The config with the launch mode set for every agent. */
export function withLaunchMode(config: AppConfig, mode: LaunchMode): AppConfig {
  return { ...config, agents: { ...config.agents, launchMode: mode } };
}
