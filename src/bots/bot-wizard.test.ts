import { describe, expect, it } from "vitest";

import {
  EMPTY_DRAFT,
  canAdvance,
  toNewBot,
  type WizardDraft,
} from "./bot-wizard";

const READY: WizardDraft = {
  ...EMPTY_DRAFT,
  name: "  Bento  ",
  instructions: "  Você organiza a rotina.  ",
};

describe("the bot wizard", () => {
  it("needs a name with a letter or a digit, up to 40 characters", () => {
    expect(canAdvance("name", EMPTY_DRAFT)).toBe(false);
    expect(canAdvance("name", { ...EMPTY_DRAFT, name: "   " })).toBe(false);
    expect(canAdvance("name", { ...EMPTY_DRAFT, name: "!!!" })).toBe(false);
    expect(canAdvance("name", { ...EMPTY_DRAFT, name: "Bento" })).toBe(true);
    expect(canAdvance("name", { ...EMPTY_DRAFT, name: "a".repeat(41) })).toBe(
      false,
    );
  });

  it("lets the engine step go on only with an engine Lynx can reach", () => {
    expect(canAdvance("engine", EMPTY_DRAFT)).toBe(true);
    expect(canAdvance("engine", { ...EMPTY_DRAFT, provider: "chatgpt" })).toBe(
      false,
    );
  });

  it("needs instructions before the review", () => {
    expect(canAdvance("instructions", EMPTY_DRAFT)).toBe(false);
    expect(canAdvance("instructions", READY)).toBe(true);
    expect(canAdvance("review", { ...READY, instructions: "  " })).toBe(false);
    expect(canAdvance("review", READY)).toBe(true);
  });

  it("makes the request from the trimmed answers, with no model when none was named", () => {
    expect(toNewBot(READY)).toEqual({
      name: "Bento",
      color: "rose",
      provider: "claude",
      model: null,
      instructions: "Você organiza a rotina.",
      team: null,
    });
    expect(toNewBot({ ...READY, model: " claude-x " }).model).toBe("claude-x");
  });

  it("takes a team only when one is named, and refuses one longer than 24 characters", () => {
    expect(toNewBot({ ...READY, team: "  infra " }).team).toBe("infra");
    expect(canAdvance("name", { ...READY, team: "a".repeat(25) })).toBe(false);
    expect(canAdvance("name", { ...READY, team: "infra" })).toBe(true);
  });
});
