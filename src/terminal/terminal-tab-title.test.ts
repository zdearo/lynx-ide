import { describe, expect, it } from "vitest";

import { session } from "@/test-support";
import { tabTitles } from "@/terminal/terminal-tab-title";

describe("tabTitles", () => {
  it("names a plain terminal after its shell and an agent after its program", () => {
    const titles = tabTitles([
      session("a", "/w"),
      session("b", "/w", { command: "claude --resume", harness: "claude" }),
    ]);
    expect(titles.get("a")).toBe("zsh");
    expect(titles.get("b")).toBe("claude");
  });

  it("numbers repeats in creation order", () => {
    const titles = tabTitles([
      session("a", "/w", { command: "claude", harness: "claude" }),
      session("b", "/w"),
      session("c", "/w", { command: "claude", harness: "claude" }),
      session("d", "/w", { command: "claude", harness: "claude" }),
    ]);
    expect([...titles.values()]).toEqual([
      "claude",
      "zsh",
      "claude 2",
      "claude 3",
    ]);
  });
});

describe("tabTitles for a shell that became an agent", () => {
  it("is named after its harness, since it was never started with a command", () => {
    const titles = tabTitles([session("a", "/w", { harness: "claude" })]);
    expect(titles.get("a")).toBe("claude");
  });
});
