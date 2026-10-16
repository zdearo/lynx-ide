import { describe, expect, it } from "vitest";

import { session } from "@/test-support";
import { isAgentCommand } from "@/terminal/agent-presets";
import { tabTitles } from "@/terminal/terminal-tab-title";

describe("tabTitles", () => {
  it("names a plain terminal after its shell and an agent after its program", () => {
    const titles = tabTitles([
      session("a", "/w"),
      session("b", "/w", { command: "claude --resume" }),
    ]);
    expect(titles.get("a")).toBe("zsh");
    expect(titles.get("b")).toBe("claude");
  });

  it("numbers repeats in creation order", () => {
    const titles = tabTitles([
      session("a", "/w", { command: "claude" }),
      session("b", "/w"),
      session("c", "/w", { command: "claude" }),
      session("d", "/w", { command: "claude" }),
    ]);
    expect([...titles.values()]).toEqual([
      "claude",
      "zsh",
      "claude 2",
      "claude 3",
    ]);
  });
});

describe("isAgentCommand", () => {
  it("recognises the known agents by program name", () => {
    expect(isAgentCommand("claude")).toBe(true);
    expect(isAgentCommand("  codex --model x")).toBe(true);
  });

  it("does not treat shells or other commands as agents", () => {
    expect(isAgentCommand(null)).toBe(false);
    expect(isAgentCommand("")).toBe(false);
    expect(isAgentCommand("npm run dev")).toBe(false);
    expect(isAgentCommand("claudette")).toBe(false);
  });
});
