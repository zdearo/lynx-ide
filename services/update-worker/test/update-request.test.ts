import { describe, expect, it } from "vitest";
import { parseUpdateRequest } from "../src/update-request";

function parse(path: string) {
  return parseUpdateRequest(new URL(`https://updates.test${path}`));
}

describe("parseUpdateRequest", () => {
  it("reads target, arch and current version, defaulting to stable", () => {
    expect(parse("/api/update/linux/x86_64/0.1.0")).toEqual({
      ok: true,
      value: {
        target: "linux",
        arch: "x86_64",
        currentVersion: "0.1.0",
        channel: "stable",
      },
    });
  });

  it("accepts an explicit stable channel", () => {
    expect(parse("/api/update/darwin/aarch64/0.1.0?channel=stable").ok).toBe(
      true,
    );
  });

  it("rejects unknown targets, architectures and channels", () => {
    expect(parse("/api/update/plan9/x86_64/0.1.0").ok).toBe(false);
    expect(parse("/api/update/linux/sparc/0.1.0").ok).toBe(false);
    expect(parse("/api/update/linux/x86_64/0.1.0?channel=beta").ok).toBe(false);
  });

  it("rejects a current version that is not semver", () => {
    expect(parse("/api/update/linux/x86_64/latest").ok).toBe(false);
  });

  it("rejects paths with missing or extra segments", () => {
    expect(parse("/api/update/linux/x86_64/0.1.0/").ok).toBe(false);
    expect(parse("/api/update/linux/0.1.0").ok).toBe(false);
  });
});
