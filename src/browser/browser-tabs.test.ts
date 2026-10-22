import { describe, expect, it } from "vitest";

import type { BrowserPageInfo, PageOwner } from "@/bindings";
import { agentName, isHarnessId, worktreePages } from "@/browser/browser-tabs";
import { copy } from "@/ui-copy";

const page = (id: number, owner: PageOwner): BrowserPageInfo => ({
  id,
  owner,
  url: "",
  title: "",
  loading: false,
  agent: null,
  detached: false,
  agentPage: false,
});

const mine: PageOwner = { kind: "workspace", worktreePath: "/work/a" };
const other: PageOwner = { kind: "workspace", worktreePath: "/work/b" };
const bot: PageOwner = { kind: "bot", id: "bot-1" };

describe("worktreePages", () => {
  it("keeps only this worktree's tabs, oldest first", () => {
    const pages = [page(3, mine), page(1, other), page(2, mine), page(4, bot)];

    expect(worktreePages(pages, "/work/a").map((p) => p.id)).toEqual([2, 3]);
  });

  it("does not reorder the list it was given", () => {
    const pages = [page(2, mine), page(1, mine)];

    worktreePages(pages, "/work/a");

    expect(pages.map((p) => p.id)).toEqual([2, 1]);
  });

  it("is empty for a worktree with no tabs", () => {
    expect(worktreePages([page(1, mine)], "/work/c")).toEqual([]);
  });
});

describe("isHarnessId", () => {
  it("accepts the harnesses the icon set draws", () => {
    expect(isHarnessId("claude")).toBe(true);
    expect(isHarnessId("omp")).toBe(true);
  });

  it("refuses any other name, including ones Object.prototype has", () => {
    expect(isHarnessId("gemini")).toBe(false);
    expect(isHarnessId("constructor")).toBe(false);
  });
});

describe("agentName", () => {
  it("names the agents the panel has a name for", () => {
    expect(agentName("claude")).toBe(copy.browser.agentNames.claude);
    expect(agentName("codex")).toBe(copy.browser.agentNames.codex);
  });

  it("falls back to the generic name for anything else", () => {
    expect(agentName("cursor")).toBe(copy.browser.agentNames.unknown);
    expect(agentName("toString")).toBe(copy.browser.agentNames.unknown);
  });
});
