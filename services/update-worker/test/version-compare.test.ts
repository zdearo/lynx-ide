import { describe, expect, it } from "vitest";
import { compareVersions, parseVersion } from "../src/version-compare";

function cmp(a: string, b: string): number {
  const left = parseVersion(a);
  const right = parseVersion(b);
  if (left === null || right === null) {
    throw new Error(`invalid version: ${a} or ${b}`);
  }
  return Math.sign(compareVersions(left, right));
}

describe("compareVersions", () => {
  it("compares the numeric core, not the text", () => {
    expect(cmp("0.10.0", "0.9.9")).toBe(1);
    expect(cmp("1.2.3", "1.2.3")).toBe(0);
  });

  it("ranks a release above its prereleases", () => {
    expect(cmp("1.0.0", "1.0.0-rc.1")).toBe(1);
  });

  it("follows the semver precedence example", () => {
    const ordered = [
      "1.0.0-alpha",
      "1.0.0-alpha.1",
      "1.0.0-alpha.beta",
      "1.0.0-beta",
      "1.0.0-beta.2",
      "1.0.0-beta.11",
      "1.0.0-rc.1",
      "1.0.0",
    ];
    for (let i = 1; i < ordered.length; i++) {
      expect(cmp(ordered[i - 1], ordered[i])).toBe(-1);
    }
  });

  it("ignores build metadata", () => {
    expect(cmp("1.0.0+a", "1.0.0+b")).toBe(0);
  });
});

describe("parseVersion", () => {
  it("rejects anything that is not strict semver", () => {
    for (const text of ["1.0", "v1.0.0", "01.0.0", "1.0.0-", ""]) {
      expect(parseVersion(text)).toBeNull();
    }
  });
});
