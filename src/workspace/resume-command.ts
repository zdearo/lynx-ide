import type { LaunchMode } from "@/bindings";
import {
  type LaunchArguments,
  launchCommand,
  type TerminalPreset,
} from "@/terminal/agent-presets";

/**
 * The line that resumes an agent's conversation: the command it is launched with, then the
 * arguments that name the conversation. Null for a plain shell, which has no conversation.
 */
export function resumeCommand(
  preset: TerminalPreset,
  launchArguments: LaunchArguments,
  mode: LaunchMode,
  resumeArguments: readonly string[],
): string | null {
  const launch = launchCommand(preset, launchArguments, mode);
  return launch === null ? null : [launch, ...resumeArguments].join(" ");
}
