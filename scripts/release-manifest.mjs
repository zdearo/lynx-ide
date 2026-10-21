// Builds the update manifest for one release. CI runs it before anything is uploaded:
//   bun scripts/release-manifest.mjs --dist <dir> --tag v0.1.1 --out <dir> [--current <stable.json>] [--notes <file>]
import { createHash } from "node:crypto";
import {
  createReadStream,
  mkdirSync,
  readFileSync,
  readdirSync,
  writeFileSync,
} from "node:fs";
import { dirname, join } from "node:path";
import { fileURLToPath, pathToFileURL } from "node:url";
import { validateManifest } from "../services/update-worker/src/release-manifest.ts";

const ROOT = join(dirname(fileURLToPath(import.meta.url)), "..");

export const DOWNLOAD_BASE = "https://downloads.zdearo.dev";
const DOWNLOAD_HOSTS = ["downloads.zdearo.dev"];

// The installer each platform's updater replaces. Tauri names them, so match by suffix.
const PLATFORMS = [
  { key: "linux-x86_64", installer: (name) => name.endsWith(".AppImage") },
  { key: "windows-x86_64", installer: (name) => name.endsWith("-setup.exe") },
];

export async function buildManifest({
  dist,
  version,
  tag,
  notes = "",
  current = null,
  now = new Date(),
}) {
  if (!isPlainVersion(version)) {
    throw new Error(`version ${version} is not a plain x.y.z`);
  }
  if (tag !== `v${version}`) {
    throw new Error(`tag ${tag} does not match version ${version}`);
  }

  const files = readdirSync(dist);
  const platforms = {};
  for (const { key, installer } of PLATFORMS) {
    const matches = files.filter(installer);
    if (matches.length !== 1) {
      throw new Error(
        `${key}: expected one installer, found ${matches.length}`,
      );
    }
    const [name] = matches;
    const signature = readFileSync(join(dist, `${name}.sig`), "utf8").trim();
    if (!isMinisignature(signature)) {
      throw new Error(`${key}: ${name}.sig is not a minisign signature`);
    }
    platforms[key] = {
      url: `${DOWNLOAD_BASE}/v${version}/${name}`,
      signature,
      sha256: await sha256File(join(dist, name)),
    };
  }

  const manifest = {
    version,
    pub_date: now.toISOString(),
    notes,
    status: "available",
    platforms,
  };

  // The same rules the Worker applies on every read, so a refused manifest never ships.
  const checked = validateManifest(manifest, DOWNLOAD_HOSTS);
  if (!checked.ok) throw new Error(`manifest refused: ${checked.reason}`);

  if (current !== null) {
    if (!isPlainVersion(current.version)) {
      throw new Error(`stable channel has version ${current.version}`);
    }
    if (compareVersions(version, current.version) <= 0) {
      throw new Error(
        `release ${version} is not newer than the stable channel (${current.version})`,
      );
    }
  }

  return manifest;
}

function isPlainVersion(text) {
  return /^\d+\.\d+\.\d+$/.test(text);
}

// The updater signs with minisign, and the .sig file is that signature in base64.
function isMinisignature(signature) {
  return Buffer.from(signature, "base64")
    .toString("utf8")
    .startsWith("untrusted comment:");
}

function compareVersions(a, b) {
  const x = a.split(".").map(Number);
  const y = b.split(".").map(Number);
  for (let i = 0; i < 3; i++) {
    if (x[i] !== y[i]) return x[i] - y[i];
  }
  return 0;
}

function sha256File(path) {
  return new Promise((resolve, reject) => {
    const hash = createHash("sha256");
    createReadStream(path)
      .on("data", (chunk) => hash.update(chunk))
      .on("end", () => resolve(hash.digest("hex")))
      .on("error", reject);
  });
}

function parseArgs(argv) {
  const args = {};
  for (let i = 0; i < argv.length; i += 2) {
    if (!argv[i].startsWith("--")) {
      throw new Error(`unexpected argument ${argv[i]}`);
    }
    args[argv[i].slice(2)] = argv[i + 1];
  }
  return args;
}

async function main(argv) {
  const args = parseArgs(argv);
  for (const flag of ["dist", "tag", "out"]) {
    if (!args[flag]) throw new Error(`missing --${flag}`);
  }

  const config = JSON.parse(
    readFileSync(join(ROOT, "src-tauri", "tauri.conf.json"), "utf8"),
  );
  const current = args.current
    ? JSON.parse(readFileSync(args.current, "utf8"))
    : null;
  const notes = args.notes ? readFileSync(args.notes, "utf8").trim() : "";

  const manifest = await buildManifest({
    dist: args.dist,
    version: config.version,
    tag: args.tag,
    notes,
    current,
  });

  mkdirSync(args.out, { recursive: true });
  writeFileSync(
    join(args.out, "manifest.json"),
    `${JSON.stringify(manifest, null, 2)}\n`,
  );
  console.log(
    `manifest ${manifest.version} for ${Object.keys(manifest.platforms).join(", ")}`,
  );
}

if (
  process.argv[1] &&
  import.meta.url === pathToFileURL(process.argv[1]).href
) {
  main(process.argv.slice(2)).catch((error) => {
    console.error(error.message);
    process.exitCode = 1;
  });
}
