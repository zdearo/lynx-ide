#!/usr/bin/env bun
// Installs the browser engine Lynx runs in its own process: downloads the CEF runtime, builds
// `lynx-browser-host`, and assembles both in `<LYNX_HOME>/engine/current`.
//
//   bun run browser:engine                 download CEF if needed, build the host (debug)
//   bun run browser:engine -- --release    build the host in release mode
//   bun run browser:engine -- --cef-dir D  use an already unpacked CEF folder instead
//   bun run browser:engine -- --bundle     also build the package the app downloads (release build)
//   bun run browser:engine -- --bundle --out D  put the package and its manifest in D
//
// For development the runtime files in `current` are links into the unpacked CEF folder. The
// package holds copies, stripped of debug symbols, with only the en-US locale, next to a manifest
// with its version, protocol and SHA-256. Publish both files; the app reads the manifest first.

import { spawnSync } from "node:child_process";
import { createHash } from "node:crypto";
import { createReadStream, createWriteStream } from "node:fs";
import {
  chmod,
  cp,
  copyFile,
  link,
  mkdir,
  readFile,
  readdir,
  rename,
  rm,
  stat,
  symlink,
  writeFile,
} from "node:fs/promises";
import { homedir, arch, platform } from "node:os";
import { dirname, join, resolve } from "node:path";
import { Readable } from "node:stream";
import { pipeline } from "node:stream/promises";
import { fileURLToPath } from "node:url";

const root = resolve(dirname(fileURLToPath(import.meta.url)), "..");
const tauriDir = join(root, "src-tauri");
const lynxHome = process.env.LYNX_HOME ?? join(homedir(), ".lynx");
const engineRoot = join(lynxHome, "engine");
const index = "https://cef-builds.spotifycdn.com/index.json";
const windows = platform() === "win32";
const hostName = windows ? "lynx-browser-host.exe" : "lynx-browser-host";
// The platform as the CEF index and the manifest name it.
const enginePlatform = windows ? "windows-x64" : "linux-x64";
// The engine's own library, which tells a CEF folder from any other.
const libraryName = windows ? "libcef.dll" : "libcef.so";

const args = process.argv.slice(2);
const makeBundle = args.includes("--bundle");
const release = args.includes("--release") || makeBundle;
const outFlag = args.includes("--out")
  ? args[args.indexOf("--out") + 1]
  : undefined;
const cefDirFlag = args.includes("--cef-dir")
  ? args[args.indexOf("--cef-dir") + 1]
  : undefined;

function fail(message) {
  console.error(`browser-engine: ${message}`);
  process.exit(1);
}

// The CEF build the Rust bindings were made for: the crate version is "<binding>+<cef>".
async function cefVersion() {
  const lock = await readFile(join(tauriDir, "Cargo.lock"), "utf8");
  const match = lock.match(/name = "cef"\nversion = "[^+"]+\+([^"]+)"/);
  if (!match) fail("the cef crate is not in src-tauri/Cargo.lock");
  return match[1];
}

// The protocol the app and the host speak, from the crate that defines it. An engine records it,
// and the app refuses one that speaks another.
async function protocolVersion() {
  const source = await readFile(
    join(tauriDir, "crates", "browser-protocol", "src", "message.rs"),
    "utf8",
  );
  const match = source.match(/PROTOCOL_VERSION: u32 = (\d+);/);
  if (!match) fail("PROTOCOL_VERSION is not in the protocol crate");
  return Number(match[1]);
}

function cefPlatform() {
  if (platform() === "linux" && arch() === "x64") return "linux64";
  if (platform() === "win32" && arch() === "x64") return "windows64";
  fail(
    `${platform()}/${arch()} is not supported yet: the host runs on Linux x64 and Windows x64 only`,
  );
}

async function sha1(path) {
  const hash = createHash("sha1");
  await pipeline(createReadStream(path), hash);
  return hash.digest("hex");
}

async function exists(path) {
  return stat(path).then(
    () => true,
    () => false,
  );
}

async function download(version) {
  const response = await fetch(index);
  if (!response.ok) fail(`the CEF index answered ${response.status}`);
  const builds = (await response.json())[cefPlatform()].versions;
  const build = builds.find((entry) =>
    entry.cef_version.startsWith(`${version}+`),
  );
  if (!build) fail(`CEF ${version} is not in the index`);
  const file = build.files.find((entry) => entry.type === "minimal");
  if (!file)
    fail(`CEF ${version} has no minimal distribution for this platform`);

  const downloads = join(engineRoot, "downloads");
  await mkdir(downloads, { recursive: true });
  const archive = join(downloads, file.name);
  if (!(await exists(archive)) || (await sha1(archive)) !== file.sha1) {
    console.log(`downloading ${file.name} (${Math.round(file.size / 1e6)} MB)`);
    const body = await fetch(
      `https://cef-builds.spotifycdn.com/${encodeURIComponent(file.name)}`,
    );
    if (!body.ok) fail(`the download answered ${body.status}`);
    await pipeline(Readable.fromWeb(body.body), createWriteStream(archive));
    if ((await sha1(archive)) !== file.sha1) {
      await rm(archive);
      fail("the download does not match its published SHA-1");
    }
  }
  console.log("CEF archive verified (SHA-1)");
  return archive;
}

async function unpack(archive, version) {
  const target = join(engineRoot, `cef-${version}`);
  if (await exists(join(target, "Release", libraryName))) return target;
  console.log(`unpacking into ${target}`);
  const partial = `${target}.partial`;
  await rm(partial, { recursive: true, force: true });
  await mkdir(partial, { recursive: true });
  const result = spawnSync(
    "tar",
    ["-xjf", archive, "-C", partial, "--strip-components=1"],
    {
      stdio: "inherit",
    },
  );
  if (result.status !== 0)
    fail("could not unpack the archive (is bzip2 installed?)");
  await rename(partial, target);
  return target;
}

function buildHost(cefDir) {
  console.log(`building ${hostName} (${release ? "release" : "debug"})`);
  const result = spawnSync(
    "cargo",
    [
      "build",
      "--manifest-path",
      join(tauriDir, "Cargo.toml"),
      "-p",
      "lynx-browser-host",
      ...(release ? ["--release"] : []),
    ],
    { stdio: "inherit", env: { ...process.env, CEF_PATH: cefDir } },
  );
  if (result.status !== 0) fail("the host did not build");
  return join(tauriDir, "target", release ? "release" : "debug", hostName);
}

// What is left out of the engine: the sandbox helper (the sandbox is off until the engine ships
// one), and the files only used to link or debug.
function skipped(name) {
  return (
    name === "chrome-sandbox" ||
    /\.(lib|pdb|dbg)$/.test(name) ||
    /^bootstrapc?\.exe$/.test(name)
  );
}

// Links need privileges on Windows, so the development layout uses hard links, and copies where
// a hard link cannot reach (another drive).
async function linkTree(from, to) {
  if ((await stat(from)).isDirectory()) {
    await mkdir(to, { recursive: true });
    for (const name of await readdir(from)) {
      await linkTree(join(from, name), join(to, name));
    }
    return;
  }
  await link(from, to).catch(() => copyFile(from, to));
}

// The host sits beside the runtime, as Chromium expects: the library, resources and locales.
async function assemble(cefDir, host, version, protocol) {
  const current = join(engineRoot, "current");
  const partial = `${current}.partial`;
  await rm(partial, { recursive: true, force: true });
  await mkdir(partial, { recursive: true });
  await copyFile(host, join(partial, hostName));
  await chmod(join(partial, hostName), 0o755);
  for (const folder of ["Release", "Resources"]) {
    for (const name of await readdir(join(cefDir, folder))) {
      if (skipped(name)) continue;
      const from = join(cefDir, folder, name);
      if (windows) await linkTree(from, join(partial, name));
      else await symlink(from, join(partial, name));
    }
  }
  await writeFile(
    join(partial, "engine.json"),
    JSON.stringify(
      {
        version: `${version}-p${protocol}`,
        protocol,
        cef: version,
        platform: enginePlatform,
        host: hostName,
        builtAt: new Date().toISOString(),
      },
      null,
      2,
    ),
  );
  await rm(current, { recursive: true, force: true });
  await rename(partial, current);
  return current;
}

// The package the app downloads: copies, not links, stripped, with the en-US locale only.
async function bundle(cefDir, host, version, protocol) {
  const out = outFlag ? resolve(outFlag) : join(engineRoot, "bundles");
  const stage = join(engineRoot, "bundle-stage");
  await rm(stage, { recursive: true, force: true });
  await mkdir(stage, { recursive: true });
  await mkdir(out, { recursive: true });

  await copyFile(host, join(stage, hostName));
  await chmod(join(stage, hostName), 0o755);
  for (const folder of ["Release", "Resources"]) {
    for (const name of await readdir(join(cefDir, folder))) {
      if (skipped(name)) continue;
      if (name === "locales") {
        await mkdir(join(stage, "locales"), { recursive: true });
        await copyFile(
          join(cefDir, folder, "locales", "en-US.pak"),
          join(stage, "locales", "en-US.pak"),
        );
        continue;
      }
      await cp(join(cefDir, folder, name), join(stage, name), {
        recursive: true,
      });
    }
  }
  for (const name of await readdir(stage)) {
    if (!windows && /\.so(\.\d+)*$/.test(name)) {
      // Debug symbols are most of libcef's size. A missing `strip` only makes the package bigger.
      spawnSync("strip", ["--strip-unneeded", join(stage, name)], {
        stdio: "ignore",
      });
    }
  }
  const id = `${version}-p${protocol}`;
  await writeFile(
    join(stage, "engine.json"),
    JSON.stringify(
      {
        version: id,
        protocol,
        cef: version,
        platform: enginePlatform,
        host: hostName,
        builtAt: new Date().toISOString(),
      },
      null,
      2,
    ),
  );

  const archiveName = `lynx-browser-engine-${enginePlatform}-${id}.tar.gz`;
  const archive = join(out, archiveName);
  console.log(`packing ${archiveName}`);
  const files = await readdir(stage);
  const packed = spawnSync("tar", ["-czf", archive, "-C", stage, ...files], {
    stdio: "inherit",
  });
  if (packed.status !== 0) fail("could not pack the engine");
  const sha256 = createHash("sha256");
  await pipeline(createReadStream(archive), sha256);
  const manifest = {
    version: id,
    protocol,
    platform: enginePlatform,
    cef: version,
    archive: archiveName,
    sha256: sha256.digest("hex"),
    size: (await stat(archive)).size,
  };
  const manifestPath = join(
    out,
    `browser-engine-manifest-${enginePlatform}.json`,
  );
  await writeFile(manifestPath, `${JSON.stringify(manifest, null, 2)}\n`);
  await rm(stage, { recursive: true, force: true });
  console.log(
    `package: ${archive} (${Math.round(manifest.size / 1e6)} MB)\nmanifest: ${manifestPath}`,
  );
}

const version = await cefVersion();
const protocol = await protocolVersion();
await mkdir(engineRoot, { recursive: true });
const cefDir = cefDirFlag
  ? resolve(cefDirFlag)
  : await unpack(await download(version), version);
if (!(await exists(join(cefDir, "Release", libraryName))))
  fail(`${cefDir} is not a CEF folder`);
const host = buildHost(cefDir);
const current = await assemble(cefDir, host, version, protocol);
console.log(`engine ready: ${current}`);
if (makeBundle) await bundle(cefDir, host, version, protocol);
