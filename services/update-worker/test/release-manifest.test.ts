import { describe, expect, it } from "vitest";
import { validateManifest } from "../src/release-manifest";

const HOSTS = ["updates-staging.test"];
const SHA = "a".repeat(64);

function artifact(overrides: Record<string, unknown> = {}) {
  return {
    url: "https://updates-staging.test/v0.1.1/Lynx.AppImage",
    signature: "sig",
    sha256: SHA,
    ...overrides,
  };
}

function manifest(overrides: Record<string, unknown> = {}) {
  return {
    version: "0.1.1",
    pub_date: "2026-10-21T12:00:00Z",
    notes: "Notas",
    status: "available",
    platforms: { "linux-x86_64": artifact() },
    ...overrides,
  };
}

describe("validateManifest", () => {
  it("accepts a complete manifest", () => {
    expect(validateManifest(manifest(), HOSTS).ok).toBe(true);
  });

  it("rejects a value that is not an object", () => {
    expect(validateManifest("oops", HOSTS).ok).toBe(false);
  });

  it("rejects download URLs that are not https or not on an allowed host", () => {
    const plain = manifest({
      platforms: {
        "linux-x86_64": artifact({ url: "http://updates-staging.test/a" }),
      },
    });
    const elsewhere = manifest({
      platforms: { "linux-x86_64": artifact({ url: "https://evil.test/a" }) },
    });
    expect(validateManifest(plain, HOSTS).ok).toBe(false);
    expect(validateManifest(elsewhere, HOSTS).ok).toBe(false);
  });

  it("rejects a platform without a signature", () => {
    const unsigned = manifest({
      platforms: { "linux-x86_64": artifact({ signature: "  " }) },
    });
    expect(validateManifest(unsigned, HOSTS).ok).toBe(false);
  });

  it("rejects a platform key the updater never asks for", () => {
    const unknown = manifest({ platforms: { "freebsd-x86_64": artifact() } });
    expect(validateManifest(unknown, HOSTS).ok).toBe(false);
  });

  it("rejects an unknown status", () => {
    expect(validateManifest(manifest({ status: "shipped" }), HOSTS).ok).toBe(
      false,
    );
  });

  it("rejects a sha256 that is not 64 hex digits", () => {
    const short = manifest({
      platforms: { "linux-x86_64": artifact({ sha256: "abc" }) },
    });
    expect(validateManifest(short, HOSTS).ok).toBe(false);
  });

  it("rejects a manifest with no platforms", () => {
    expect(validateManifest(manifest({ platforms: {} }), HOSTS).ok).toBe(false);
  });

  it("rejects a version that is not semver", () => {
    expect(validateManifest(manifest({ version: "0.1" }), HOSTS).ok).toBe(
      false,
    );
  });
});
