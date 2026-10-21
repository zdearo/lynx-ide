import { describe, expect, it } from "vitest";
import type { ReleaseManifest } from "../src/release-manifest";
import { decideUpdate } from "../src/update-decision";
import type { UpdateRequest } from "../src/update-request";

const request: UpdateRequest = {
  target: "linux",
  arch: "x86_64",
  currentVersion: "0.1.0",
  channel: "stable",
};

const release: ReleaseManifest = {
  version: "0.1.1",
  pub_date: "2026-10-21T12:00:00Z",
  notes: "Notas",
  status: "available",
  platforms: {
    "linux-x86_64": {
      url: "https://updates-staging.test/v0.1.1/Lynx.AppImage",
      signature: "sig",
      sha256: "a".repeat(64),
    },
  },
};

describe("decideUpdate", () => {
  it("offers a newer version that has an artifact for the platform", () => {
    expect(decideUpdate(request, release)).toEqual({
      kind: "offer",
      offer: {
        version: "0.1.1",
        url: "https://updates-staging.test/v0.1.1/Lynx.AppImage",
        signature: "sig",
        notes: "Notas",
        pub_date: "2026-10-21T12:00:00Z",
      },
    });
  });

  it("offers nothing when the app is already on that version or newer", () => {
    const same = decideUpdate({ ...request, currentVersion: "0.1.1" }, release);
    const newer = decideUpdate(
      { ...request, currentVersion: "0.2.0" },
      release,
    );
    expect(same).toEqual({ kind: "none", reason: "up-to-date" });
    expect(newer).toEqual({ kind: "none", reason: "up-to-date" });
  });

  it("holds back a release that is held or withdrawn", () => {
    expect(decideUpdate(request, { ...release, status: "held" })).toEqual({
      kind: "none",
      reason: "held",
    });
    expect(decideUpdate(request, { ...release, status: "withdrawn" })).toEqual({
      kind: "none",
      reason: "withdrawn",
    });
  });

  it("offers nothing for a platform the release does not build", () => {
    expect(decideUpdate({ ...request, arch: "aarch64" }, release)).toEqual({
      kind: "none",
      reason: "no-artifact",
    });
  });
});
