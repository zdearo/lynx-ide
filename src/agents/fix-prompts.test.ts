import { describe, expect, it } from "vitest";

import {
  agentCommand,
  brokenChecksPrompt,
  commitFailurePrompt,
  pickFixAgent,
  promptInstruction,
} from "@/agents/fix-prompts";
import { TERMINAL_PRESETS } from "@/terminal/agent-presets";

const preset = (id: string) => {
  const found = TERMINAL_PRESETS.find((candidate) => candidate.id === id);
  if (!found) throw new Error(`no preset ${id}`);
  return found;
};

describe("commitFailurePrompt", () => {
  it("carries the attempt, the files and git's own words, and forbids bypassing hooks", () => {
    const prompt = commitFailurePrompt({
      error: "eslint: 2 errors",
      summary: "fix: x",
      description: "Because.",
      files: ["src/a.ts", "src/b.ts"],
    });

    expect(prompt).toContain("fix: x\n\nBecause.");
    expect(prompt).toContain("- src/a.ts\n- src/b.ts");
    expect(prompt).toContain("eslint: 2 errors");
    expect(prompt).toContain("Do not bypass hooks");
    expect(prompt).toContain("Do not commit, push");
  });
});

describe("brokenChecksPrompt", () => {
  it("lists each failing check with its link and says how to read the log", () => {
    const prompt = brokenChecksPrompt({
      number: 42,
      title: "Fix auth",
      branch: "feat/auth",
      checks: [
        {
          name: "CI / test",
          url: "https://github.com/a/b/actions/runs/1/job/2",
        },
        { name: "deploy", url: null },
      ],
    });

    expect(prompt).toContain('#42 "Fix auth" (branch feat/auth)');
    expect(prompt).toContain(
      "- CI / test: https://github.com/a/b/actions/runs/1/job/2",
    );
    expect(prompt).toContain("- deploy\n".trimEnd());
    expect(prompt).toContain("gh run view --job");
  });
});

describe("pickFixAgent", () => {
  const all = [
    preset("claude"),
    preset("codex"),
    preset("opencode"),
    preset("cursor"),
  ];

  it("takes the configured agent when it is on, else the first of the three", () => {
    expect(pickFixAgent(all, "codex")?.id).toBe("codex");
    expect(pickFixAgent(all, "auto")?.id).toBe("claude");
    expect(
      pickFixAgent([preset("codex"), preset("opencode")], "claude")?.id,
    ).toBe("codex");
  });

  it("offers nothing when none of the three is on, even if another agent is", () => {
    expect(
      pickFixAgent([preset("cursor"), preset("shell")], "auto"),
    ).toBeNull();
  });
});

describe("agentCommand", () => {
  const instruction = promptInstruction("/tmp/lynx-prompts/prompt-1.md");

  it("puts the instruction after the command, quoted for the shell", () => {
    expect(
      agentCommand(preset("claude"), {}, "manual", instruction, false),
    ).toBe(
      "claude 'Read /tmp/lynx-prompts/prompt-1.md and follow the instructions in it.'",
    );
  });

  it("gives OpenCode its prompt through --prompt, after the launch arguments", () => {
    expect(
      agentCommand(
        preset("opencode"),
        { opencode: "--model x" },
        "manual",
        instruction,
        false,
      ),
    ).toBe(
      "opencode --model x --prompt 'Read /tmp/lynx-prompts/prompt-1.md and follow the instructions in it.'",
    );
  });

  it("keeps the YOLO flag the launch mode asks for", () => {
    expect(agentCommand(preset("claude"), {}, "yolo", "go", false)).toBe(
      "claude --dangerously-skip-permissions go",
    );
  });
});
