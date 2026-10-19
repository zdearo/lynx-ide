import { describe, expect, it } from "vitest";

import {
  isValidOwner,
  isValidRepoName,
} from "@/pull-request/create-remote-form";

describe("isValidOwner", () => {
  it("accepts GitHub account names", () => {
    expect(isValidOwner("zdearo")).toBe(true);
    expect(isValidOwner("my-org")).toBe(true);
    expect(isValidOwner("a")).toBe(true);
  });

  it("refuses empty names, edge hyphens, other characters and long names", () => {
    expect(isValidOwner("")).toBe(false);
    expect(isValidOwner("-lead")).toBe(false);
    expect(isValidOwner("trail-")).toBe(false);
    expect(isValidOwner("has_underscore")).toBe(false);
    expect(isValidOwner("a".repeat(40))).toBe(false);
  });
});

describe("isValidRepoName", () => {
  it("accepts names with letters, digits, dots, hyphens and underscores", () => {
    expect(isValidRepoName("lynx-ide")).toBe(true);
    expect(isValidRepoName("agent_ade.v2")).toBe(true);
  });

  it("refuses spaces, slashes, the dot names and empty or long names", () => {
    expect(isValidRepoName("")).toBe(false);
    expect(isValidRepoName(".")).toBe(false);
    expect(isValidRepoName("..")).toBe(false);
    expect(isValidRepoName("with space")).toBe(false);
    expect(isValidRepoName("a/b")).toBe(false);
    expect(isValidRepoName("a".repeat(101))).toBe(false);
  });
});
