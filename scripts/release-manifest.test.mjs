import { mkdtempSync, rmSync, writeFileSync } from "node:fs";
import { tmpdir } from "node:os";
import { join } from "node:path";
import { afterEach, beforeEach, describe, expect, it } from "vitest";
import { buildManifest, DOWNLOAD_BASE } from "./release-manifest.mjs";

const SIGNATURE = Buffer.from(
  "untrusted comment: signature from tauri secret key\nRUQfake\n",
).toString("base64");
const NOW = new Date("2026-10-21T12:00:00Z");

let dist;

beforeEach(() => {
  dist = mkdtempSync(join(tmpdir(), "lynx-release-"));
  writeArtifact("Lynx_0.1.1_amd64.AppImage");
  writeArtifact("Lynx_0.1.1_x64-setup.exe");
});

afterEach(() => {
  rmSync(dist, { recursive: true, force: true });
});

function writeArtifact(name, signature = SIGNATURE) {
  writeFileSync(join(dist, name), `binary ${name}`);
  writeFileSync(join(dist, `${name}.sig`), signature);
}

function build(overrides = {}) {
  return buildManifest({
    dist,
    version: "0.1.1",
    tag: "v0.1.1",
    now: NOW,
    ...overrides,
  });
}

describe("buildManifest", () => {
  it("lists each platform's installer with its signature and hash", async () => {
    const manifest = await build();

    expect(manifest.status).toBe("available");
    expect(manifest.pub_date).toBe("2026-10-21T12:00:00.000Z");
    expect(manifest.platforms["linux-x86_64"]).toMatchObject({
      url: `${DOWNLOAD_BASE}/v0.1.1/Lynx_0.1.1_amd64.AppImage`,
      signature: SIGNATURE,
    });
    expect(manifest.platforms["windows-x86_64"].url).toBe(
      `${DOWNLOAD_BASE}/v0.1.1/Lynx_0.1.1_x64-setup.exe`,
    );
    expect(manifest.platforms["linux-x86_64"].sha256).toMatch(/^[0-9a-f]{64}$/);
  });

  it("refuses a tag that does not match the version", async () => {
    await expect(build({ tag: "v0.1.2" })).rejects.toThrow(
      /does not match version/,
    );
  });

  it("refuses a version that is not plain x.y.z", async () => {
    await expect(
      build({ version: "0.1.1-rc.1", tag: "v0.1.1-rc.1" }),
    ).rejects.toThrow(/not a plain x\.y\.z/);
  });

  it("refuses a release missing one platform's installer", async () => {
    rmSync(join(dist, "Lynx_0.1.1_x64-setup.exe"));
    rmSync(join(dist, "Lynx_0.1.1_x64-setup.exe.sig"));

    await expect(build()).rejects.toThrow(
      /windows-x86_64: expected one installer, found 0/,
    );
  });

  it("refuses a signature that is not minisign", async () => {
    writeArtifact(
      "Lynx_0.1.1_amd64.AppImage",
      Buffer.from("not a signature").toString("base64"),
    );

    await expect(build()).rejects.toThrow(/not a minisign signature/);
  });

  it("refuses a release that is not newer than the stable channel", async () => {
    await expect(build({ current: { version: "0.1.1" } })).rejects.toThrow(
      /not newer than the stable channel/,
    );
    await expect(build({ current: { version: "0.2.0" } })).rejects.toThrow(
      /not newer than the stable channel/,
    );
  });

  it("accepts a newer release over the current channel", async () => {
    await expect(
      build({ current: { version: "0.1.0" } }),
    ).resolves.toMatchObject({ version: "0.1.1" });
  });

  it("accepts the first release when no channel exists yet", async () => {
    await expect(build({ current: null })).resolves.toMatchObject({
      version: "0.1.1",
    });
  });
});
