import { describe, expect, it } from "vitest";

import { TEAM_DOCS_MEMBERS, TEAM_MEMBERS } from "@/team/team-fixtures";
import {
  canOrchestrate,
  hasTask,
  linkOf,
  orchestratorOptionsOf,
} from "@/team/team-hierarchy";

describe("linkOf", () => {
  it("reads bottom to top as an orchestration, from either end", () => {
    expect(
      linkOf({
        source: "a",
        target: "b",
        sourceHandle: "bottom",
        targetHandle: "top",
      }),
    ).toEqual({ kind: "orchestrates", parent: "a", child: "b" });
    expect(
      linkOf({
        source: "b",
        target: "a",
        sourceHandle: "top",
        targetHandle: "bottom",
      }),
    ).toEqual({ kind: "orchestrates", parent: "a", child: "b" });
  });

  it("reads side to side as a pair, and refuses a mix", () => {
    expect(
      linkOf({
        source: "a",
        target: "b",
        sourceHandle: "right",
        targetHandle: "left",
      }),
    ).toEqual({ kind: "talks", from: "a", to: "b" });
    expect(
      linkOf({
        source: "a",
        target: "b",
        sourceHandle: "right",
        targetHandle: "top",
      }),
    ).toBeNull();
  });
});

describe("canOrchestrate", () => {
  it("needs orchestrator mode on the parent", () => {
    expect(canOrchestrate(TEAM_MEMBERS, "lead", "tester")).toBe(true);
    expect(canOrchestrate(TEAM_MEMBERS, "reviewer", "tester")).toBe(false);
  });

  it("refuses itself and a member's own ancestors' loop", () => {
    expect(canOrchestrate(TEAM_MEMBERS, "builder", "builder")).toBe(false);
    // Lead is above Builder, so Builder cannot take Lead as its member.
    expect(canOrchestrate(TEAM_MEMBERS, "builder", "lead")).toBe(false);
  });

  it("lists only orchestrators as options", () => {
    const options = orchestratorOptionsOf("tester", TEAM_MEMBERS);
    expect(options.map((option) => option.member.id)).toEqual([
      "lead",
      "builder",
    ]);
    expect(options.find((option) => option.member.id === "lead")?.current).toBe(
      true,
    );
  });
});

describe("hasTask", () => {
  it("gives a task only to a member with an orchestrator above it", () => {
    const byId = (id: string) => TEAM_MEMBERS.find((m) => m.id === id)!;
    expect(hasTask(byId("tester"))).toBe(true);
    expect(hasTask(byId("reviewer"))).toBe(false);
    expect(hasTask(byId("lead"))).toBe(false);
  });
});

describe("bots", () => {
  // The docs team: its Lead is a bot, and its Writer is an agent under it. Atlas is a second bot in that team.
  const docsLead = TEAM_DOCS_MEMBERS[0];
  const atlas = { ...docsLead, id: "atlas", botId: "atlas", role: "Atlas" };
  const docs = [...TEAM_DOCS_MEMBERS, atlas];
  // The writer is made an orchestrator, so the refusals below are about the bots and not about the writer.
  const orchestratingWriter = docs.map((member) =>
    member.id === "writer" ? { ...member, orchestrator: true } : member,
  );

  it("is never a child: no one orchestrates a bot", () => {
    expect(canOrchestrate(orchestratingWriter, "writer", "docs-lead")).toBe(
      false,
    );
    expect(canOrchestrate(orchestratingWriter, "docs-lead", "atlas")).toBe(
      false,
    );
  });

  it("orchestrates agents, and so does each bot", () => {
    expect(canOrchestrate(docs, "docs-lead", "writer")).toBe(true);
    expect(canOrchestrate(docs, "atlas", "writer")).toBe(true);
  });

  it("lists a bot as an orchestrator an agent can answer to", () => {
    const options = orchestratorOptionsOf("writer", docs);
    expect(options.find((o) => o.member.id === "docs-lead")).toMatchObject({
      current: true,
      enabled: true,
    });
    expect(options.find((o) => o.member.id === "atlas")).toMatchObject({
      current: false,
      enabled: true,
    });
  });

  it("has a task only under an orchestrator, so the bot has none", () => {
    expect(hasTask(docsLead)).toBe(false);
    expect(hasTask(TEAM_DOCS_MEMBERS[1])).toBe(true);
  });

  it("is the only bot in the mock, and every other member is an agent", () => {
    const all = [...TEAM_MEMBERS, ...TEAM_DOCS_MEMBERS];
    expect(
      all.filter((member) => member.kind === "bot").map((m) => m.id),
    ).toEqual(["docs-lead"]);
  });
});
