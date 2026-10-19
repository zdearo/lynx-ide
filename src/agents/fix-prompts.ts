import type { CommitMessageAgent, HarnessId, LaunchMode } from "@/bindings";
import { quotePathForShell } from "@/files/shell-quote";
import {
  launchCommand,
  type LaunchArguments,
  type TerminalPreset,
} from "@/terminal/agent-presets";

/** The agents Lynx knows how to hand a first prompt to, with how each takes it. */
const PROMPT_AGENTS: readonly HarnessId[] = ["claude", "codex", "opencode"];

const RULES = `Rules:
- Work only in this worktree.
- Do not bypass hooks (no --no-verify) and do not turn checks off.
- Do not commit, push or open a pull request. Fix the files, then say what you changed.`;

/** What an agent is told when a commit failed: the attempt, the files, and what git printed. */
export function commitFailurePrompt(failure: {
  error: string;
  summary: string;
  description: string;
  files: readonly string[];
}): string {
  const message = [failure.summary, failure.description]
    .filter((part) => part.trim() !== "")
    .join("\n\n");
  return [
    "A git commit in this repository failed. Find out why and fix it.",
    RULES,
    `Commit message that was attempted:\n${message || "(none)"}`,
    `Staged files:\n${failure.files.map((file) => `- ${file}`).join("\n") || "(none)"}`,
    `What git printed:\n\`\`\`\n${failure.error}\n\`\`\``,
  ].join("\n\n");
}

/** What an agent is told when a pull request has failing checks. */
export function brokenChecksPrompt(pullRequest: {
  number: number;
  title: string;
  branch: string;
  checks: readonly { name: string; url: string | null }[];
}): string {
  const lines = pullRequest.checks
    .map((check) => `- ${check.name}${check.url ? `: ${check.url}` : ""}`)
    .join("\n");
  return [
    `Pull request #${pullRequest.number} "${pullRequest.title}" (branch ${pullRequest.branch}) has failing checks. Find out why they fail and fix the code.`,
    RULES,
    "The log of a failing GitHub Actions job can be read with `gh run view --job <job id> --log-failed`, where the job id is the number after /job/ in the link.",
    `Failing checks:\n${lines}`,
  ].join("\n\n");
}

/** The short line typed on the agent's command line, which points it at the prompt file. */
export function promptInstruction(path: string): string {
  return `Read ${path} and follow the instructions in it.`;
}

/**
 * The agent to hand a task to: the one chosen for commit messages when it is on, else the first
 * of Claude Code, Codex and OpenCode that is on. Others are not offered: how they take a first
 * prompt is not known here.
 */
export function pickFixAgent(
  presets: readonly TerminalPreset[],
  configured: CommitMessageAgent,
): TerminalPreset | null {
  const available = PROMPT_AGENTS.map((harness) =>
    presets.find((preset) => preset.harness === harness),
  ).filter((preset): preset is TerminalPreset => preset !== undefined);
  return (
    available.find((preset) => preset.harness === configured) ??
    available[0] ??
    null
  );
}

/** The command line that starts the agent with its settings and the one-line instruction. */
export function agentCommand(
  preset: TerminalPreset,
  launchArguments: LaunchArguments,
  mode: LaunchMode,
  instruction: string,
  windows: boolean,
): string | null {
  const base = launchCommand(preset, launchArguments, mode);
  if (base === null) return null;
  const quoted = quotePathForShell(instruction, windows);
  return preset.harness === "opencode"
    ? `${base} --prompt ${quoted}`
    : `${base} ${quoted}`;
}
