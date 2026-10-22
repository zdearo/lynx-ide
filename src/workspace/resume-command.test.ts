import { describe, expect, it } from "vitest";

import { TERMINAL_PRESETS } from "@/terminal/agent-presets";
import { resumeCommand } from "@/workspace/resume-command";

const preset = (id: string) => {
  const found = TERMINAL_PRESETS.find((candidate) => candidate.id === id);
  if (!found) throw new Error(`no preset ${id}`);
  return found;
};

describe("resumeCommand", () => {
  it("resumes Claude Code by its conversation, in the mode it was launched in", () => {
    expect(
      resumeCommand(preset("claude"), {}, "manual", ["--resume", "abc-123"]),
    ).toBe("claude --resume abc-123");
    expect(
      resumeCommand(preset("claude"), {}, "yolo", ["--resume", "abc-123"]),
    ).toBe("claude --dangerously-skip-permissions --resume abc-123");
  });

  it("keeps Codex's resume subcommand after its flags", () => {
    expect(
      resumeCommand(preset("codex"), {}, "yolo", ["resume", "abc-123"]),
    ).toBe("codex --dangerously-bypass-approvals-and-sandbox resume abc-123");
  });

  it("has no resume for a plain shell", () => {
    expect(
      resumeCommand(preset("shell"), {}, "manual", ["--resume", "abc"]),
    ).toBeNull();
  });
});
