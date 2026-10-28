import { describe, expect, it } from "vitest";

import {
  TEAM_DOCS_MEMBERS,
  TEAM_MEMBERS,
  TEAM_ROOT_ID,
} from "@/team/team-fixtures";
import {
  isValidConnection,
  linkOptionsOf,
  pairKey,
  partnersOf,
  type TeamPair,
} from "@/team/team-pairs";

const pairs: TeamPair[] = [{ from: "builder", to: "reviewer" }];

describe("isValidConnection", () => {
  it("accepts two members that are neither parent and child nor paired yet", () => {
    expect(
      isValidConnection(
        { source: "researcher", target: "tester" },
        TEAM_MEMBERS,
        pairs,
      ),
    ).toBe(true);
  });

  it("refuses a member with itself, a duplicate in either direction, and a parent and child", () => {
    const check = (source: string, target: string) =>
      isValidConnection({ source, target }, TEAM_MEMBERS, pairs);
    expect(check("tester", "tester")).toBe(false);
    expect(check("builder", "reviewer")).toBe(false);
    expect(check("reviewer", "builder")).toBe(false);
    expect(check(TEAM_ROOT_ID, "builder")).toBe(false);
    expect(check("login-tests", "builder")).toBe(false);
  });
});

describe("pair helpers", () => {
  it("keys a pair the same way in both directions", () => {
    expect(pairKey("a", "b")).toBe(pairKey("b", "a"));
  });

  it("lists a member's partners from either end of a pair", () => {
    expect(partnersOf(pairs, "builder")).toEqual(["reviewer"]);
    expect(partnersOf(pairs, "reviewer")).toEqual(["builder"]);
    expect(partnersOf(pairs, "tester")).toEqual([]);
  });

  it("marks the linked members, and disables the parent and child", () => {
    const options = linkOptionsOf("builder", TEAM_MEMBERS, pairs);
    const byId = Object.fromEntries(
      options.map((option) => [option.member.id, option]),
    );
    expect(byId.reviewer).toMatchObject({ linked: true, enabled: true });
    expect(byId.lead).toMatchObject({ linked: false, enabled: false });
    expect(byId["login-tests"]).toMatchObject({
      linked: false,
      enabled: false,
    });
    expect(byId.tester).toMatchObject({ linked: false, enabled: true });
  });
});

describe("bots in pairs", () => {
  const docsLead = TEAM_DOCS_MEMBERS[0];
  const atlas = { ...docsLead, id: "atlas", botId: "atlas", role: "Atlas" };
  // An agent with no orchestrator: it can pair with the bot, since it is not the bot's member.
  const loose = { ...TEAM_DOCS_MEMBERS[1], id: "loose", parentId: null };
  const members = [...TEAM_DOCS_MEMBERS, atlas, loose];

  it("pairs an agent with a bot, and never two bots", () => {
    const check = (source: string, target: string) =>
      isValidConnection({ source, target }, members, []);
    expect(check("loose", "docs-lead")).toBe(true);
    expect(check("docs-lead", "atlas")).toBe(false);
  });
});
