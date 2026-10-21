import { createHash } from "node:crypto";
import { mkdtempSync, rmSync, writeFileSync } from "node:fs";
import { tmpdir } from "node:os";
import { join } from "node:path";
import { afterEach, beforeEach, describe, expect, it } from "vitest";
import { plan, readManifest, verifyArchive } from "./engine-publish.mjs";

const ARCHIVE = "lynx-browser-engine-linux-x64-154.0.34-p2.tar.gz";
const MANIFEST = "browser-engine-manifest-linux-x64.json";
const CONTENT = "engine bytes";

let packageDir;

function manifestFor(archive = ARCHIVE, overrides = {}) {
  return {
    version: "154.0.34-p2",
    protocol: 2,
    platform: "linux-x64",
    cef: "154.0.34",
    archive,
    sha256: createHash("sha256").update(CONTENT).digest("hex"),
    size: Buffer.byteLength(CONTENT),
    ...overrides,
  };
}

function writePackage(manifest, files = { [ARCHIVE]: CONTENT }) {
  for (const [name, content] of Object.entries(files)) {
    writeFileSync(join(packageDir, name), content);
  }
  writeFileSync(join(packageDir, MANIFEST), JSON.stringify(manifest));
}

beforeEach(() => {
  packageDir = mkdtempSync(join(tmpdir(), "lynx-engine-"));
});

afterEach(() => {
  rmSync(packageDir, { recursive: true, force: true });
});

describe("readManifest", () => {
  it("reads the one manifest in the package", () => {
    writePackage(manifestFor());
    const { file, manifest } = readManifest(packageDir);
    expect(file).toBe(MANIFEST);
    expect(manifest.version).toBe("154.0.34-p2");
    expect(manifest.archive).toBe(ARCHIVE);
  });

  it("refuses a package without a manifest, or with two", () => {
    expect(() => readManifest(packageDir)).toThrow(/found 0/);
    writePackage(manifestFor());
    writeFileSync(
      join(packageDir, "browser-engine-manifest-windows-x64.json"),
      "{}",
    );
    expect(() => readManifest(packageDir)).toThrow(/found 2/);
  });

  it("refuses an archive name that is not a plain file name", () => {
    writePackage(manifestFor("../escape.tar.gz"));
    expect(() => readManifest(packageDir)).toThrow(/not a plain file name/);
  });
});

describe("verifyArchive", () => {
  it("accepts the archive the manifest describes", async () => {
    writePackage(manifestFor());
    await expect(
      verifyArchive(packageDir, manifestFor()),
    ).resolves.toBeUndefined();
  });

  it("refuses an archive whose bytes changed, even at the same size", async () => {
    writePackage(manifestFor(), { [ARCHIVE]: "ENGINE BYTES" });
    await expect(verifyArchive(packageDir, manifestFor())).rejects.toThrow(
      /does not match its checksum/,
    );
  });

  it("refuses an archive of the wrong size", async () => {
    writePackage(manifestFor(), { [ARCHIVE]: CONTENT + "!" });
    await expect(verifyArchive(packageDir, manifestFor())).rejects.toThrow(
      /bytes, the manifest says/,
    );
  });
});

describe("plan", () => {
  it("points the pointer at the archive inside the version's folder", () => {
    const { pointer } = plan(manifestFor(), MANIFEST);
    expect(pointer.archive).toBe(`154.0.34-p2/${ARCHIVE}`);
    expect(pointer.sha256).toBe(manifestFor().sha256);
    expect(pointer.version).toBe("154.0.34-p2");
  });

  it("uploads the archive, then the version's manifest, and the pointer last", () => {
    const { uploads } = plan(manifestFor(), MANIFEST);
    expect(uploads.map((entry) => entry.key)).toEqual([
      `engine/154.0.34-p2/${ARCHIVE}`,
      `engine/154.0.34-p2/${MANIFEST}`,
      `engine/${MANIFEST}`,
    ]);
    expect(uploads[2].from).toBe("out");
  });
});
