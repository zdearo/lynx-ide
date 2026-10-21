// Publishes a browser engine package under engine/ in the release bucket. Each version gets its own
// folder, which is never overwritten. The platform's pointer moves last, so the app never reads a
// manifest whose archive is not there yet:
//   bun scripts/engine-publish.mjs --package <dir> --out <dir> [--upload]
// Without --upload it only checks the package and writes the pointer file into --out.
import { spawnSync } from "node:child_process";
import { createHash } from "node:crypto";
import {
  createReadStream,
  mkdirSync,
  readFileSync,
  readdirSync,
  statSync,
  writeFileSync,
} from "node:fs";
import { dirname, join, resolve } from "node:path";
import { fileURLToPath, pathToFileURL } from "node:url";

const ROOT = join(dirname(fileURLToPath(import.meta.url)), "..");
const WORKER = join(ROOT, "services", "update-worker");
const WRANGLER = join(WORKER, "node_modules", "wrangler", "bin", "wrangler.js");

export const BUCKET = "lynx-releases-prod";
export const ENGINE_BASE = "https://downloads.zdearo.dev/engine";

const MANIFEST_NAME = /^browser-engine-manifest-[a-z0-9-]+\.json$/;
const PLAIN_NAME = /^[A-Za-z0-9._-]+$/;

/** The package's manifest, checked: exactly one, with plain names and the fields the app reads. */
export function readManifest(packageDir) {
  const names = readdirSync(packageDir).filter((name) =>
    MANIFEST_NAME.test(name),
  );
  if (names.length !== 1) {
    throw new Error(
      `expected one engine manifest in ${packageDir}, found ${names.length}`,
    );
  }
  const [file] = names;
  const manifest = JSON.parse(readFileSync(join(packageDir, file), "utf8"));
  for (const field of ["version", "platform", "archive", "sha256", "size"]) {
    if (manifest[field] === undefined)
      throw new Error(`${file} has no ${field}`);
  }
  if (!PLAIN_NAME.test(manifest.version)) {
    throw new Error(`version ${manifest.version} is not a plain name`);
  }
  if (!PLAIN_NAME.test(manifest.archive)) {
    throw new Error(`archive ${manifest.archive} is not a plain file name`);
  }
  if (!/^[0-9a-f]{64}$/.test(manifest.sha256))
    throw new Error("sha256 is malformed");
  if (!Number.isSafeInteger(manifest.size) || manifest.size <= 0) {
    throw new Error("size is malformed");
  }
  return { file, manifest };
}

/** The archive on disk is the one the manifest describes, byte for byte. */
export async function verifyArchive(packageDir, manifest) {
  const path = join(packageDir, manifest.archive);
  const size = statSync(path).size;
  if (size !== manifest.size) {
    throw new Error(
      `${manifest.archive} is ${size} bytes, the manifest says ${manifest.size}`,
    );
  }
  if ((await sha256File(path)) !== manifest.sha256) {
    throw new Error(`${manifest.archive} does not match its checksum`);
  }
}

/**
 * What goes to the bucket, in the order it must land. The pointer names the archive relative to its
 * own place, so it points into the version's folder.
 */
export function plan(manifest, manifestFile) {
  const folder = `engine/${manifest.version}`;
  return {
    pointer: {
      ...manifest,
      archive: `${manifest.version}/${manifest.archive}`,
    },
    uploads: [
      {
        name: manifest.archive,
        from: "package",
        key: `${folder}/${manifest.archive}`,
        contentType: "application/gzip",
      },
      {
        name: manifestFile,
        from: "package",
        key: `${folder}/${manifestFile}`,
        contentType: "application/json",
      },
      {
        name: manifestFile,
        from: "out",
        key: `engine/${manifestFile}`,
        contentType: "application/json",
      },
    ],
  };
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

// A version already published is never replaced: the app may have downloaded it, and a cache may
// still hold it. A new build needs a new CEF or protocol version.
async function alreadyPublished(url) {
  const response = await fetch(url, { method: "HEAD" });
  return response.ok;
}

/**
 * The wrangler arguments that put one file in the bucket. The file path is absolute, because
 * wrangler runs from the Worker's folder, not from where this script was started.
 */
export function uploadArgs(entry, packageDir, outDir) {
  const file = resolve(entry.from === "out" ? outDir : packageDir, entry.name);
  return [
    "r2",
    "object",
    "put",
    `${BUCKET}/${entry.key}`,
    "--file",
    file,
    "--content-type",
    entry.contentType,
    "--remote",
  ];
}

function upload(uploads, packageDir, outDir) {
  for (const entry of uploads) {
    // Run by bun itself: no shell, and no .cmd shim to find on Windows.
    const result = spawnSync(
      process.execPath,
      [WRANGLER, ...uploadArgs(entry, packageDir, outDir)],
      { cwd: WORKER, stdio: "inherit" },
    );
    if (result.status !== 0) throw new Error(`upload of ${entry.key} failed`);
  }
}

function parseArgs(argv) {
  const args = { upload: false };
  for (let i = 0; i < argv.length; i++) {
    if (argv[i] === "--upload") {
      args.upload = true;
    } else if (argv[i].startsWith("--")) {
      args[argv[i].slice(2)] = argv[i + 1];
      i++;
    } else {
      throw new Error(`unexpected argument ${argv[i]}`);
    }
  }
  return args;
}

async function main(argv) {
  const args = parseArgs(argv);
  for (const flag of ["package", "out"]) {
    if (!args[flag]) throw new Error(`missing --${flag}`);
  }

  const { file, manifest } = readManifest(args.package);
  await verifyArchive(args.package, manifest);
  const { pointer, uploads } = plan(manifest, file);

  mkdirSync(args.out, { recursive: true });
  writeFileSync(join(args.out, file), `${JSON.stringify(pointer, null, 2)}\n`);

  if (!args.upload) {
    console.log(
      `checked ${manifest.archive}; would publish engine ${manifest.version}`,
    );
    return;
  }
  if (
    await alreadyPublished(
      `${ENGINE_BASE}/${manifest.version}/${manifest.archive}`,
    )
  ) {
    throw new Error(
      `engine ${manifest.version} is already published; a new build needs a new CEF or protocol version`,
    );
  }
  upload(uploads, args.package, args.out);
  console.log(`published engine ${manifest.version} for ${manifest.platform}`);
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
