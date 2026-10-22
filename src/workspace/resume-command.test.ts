import { describe, expect, it } from "vitest";

import { resumeCommand } from "@/workspace/resume-command";

describe("resumeCommand", () => {
  it("resumes Claude Code by its conversation, after the line it was launched with", () => {
    expect(resumeCommand("claude", ["--resume", "abc-123"])).toBe(
      "claude --resume abc-123",
    );
    expect(
      resumeCommand("claude --dangerously-skip-permissions", [
        "--resume",
        "abc-123",
      ]),
    ).toBe("claude --dangerously-skip-permissions --resume abc-123");
  });

  it("keeps Codex's resume subcommand after its flags", () => {
    expect(
      resumeCommand("codex --dangerously-bypass-approvals-and-sandbox", [
        "resume",
        "abc-123",
      ]),
    ).toBe("codex --dangerously-bypass-approvals-and-sandbox resume abc-123");
  });

  it("has no resume for a plain shell, which has no launch line", () => {
    expect(resumeCommand(null, ["--resume", "abc"])).toBeNull();
  });
});
