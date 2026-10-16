import { describe, expect, it } from "vitest";

import {
  choiceItems,
  choiceKey,
  isCustomValue,
  type ChoiceOption,
} from "@/settings/choice-field";

const OPTIONS: ChoiceOption[] = [
  { value: null, label: "Default" },
  { value: "zsh", label: "zsh" },
];

describe("choiceKey", () => {
  it("keeps 'not set' apart from a value that happens to read the same", () => {
    expect(choiceKey(null)).not.toBe(choiceKey("default"));
  });
});

describe("isCustomValue", () => {
  it("is false for 'not set' and for a listed value", () => {
    expect(isCustomValue(null, OPTIONS)).toBe(false);
    expect(isCustomValue("zsh", OPTIONS)).toBe(false);
  });

  it("is true for a value no entry offers", () => {
    expect(isCustomValue("/opt/fish", OPTIONS)).toBe(true);
  });
});

describe("choiceItems", () => {
  it("shows the list as it is when the value is listed", () => {
    expect(choiceItems("zsh", OPTIONS, false)).toEqual(OPTIONS);
  });

  it("adds an entry for a value the list does not offer", () => {
    expect(choiceItems("/opt/fish", OPTIONS, false)).toEqual([
      ...OPTIONS,
      { value: "/opt/fish", label: "/opt/fish" },
    ]);
  });

  it("does not add it when a custom field will show the value", () => {
    expect(choiceItems("/opt/fish", OPTIONS, true)).toEqual(OPTIONS);
  });
});
