// Writes THIRD_PARTY_LICENSES.md: the licence of every npm package and Cargo crate that goes into
// Lynx, with each licence text once. Run it after a dependency changes:
//   bun scripts/third-party-licenses.mjs
import { execFileSync } from "node:child_process";
import { createHash } from "node:crypto";
import {
  existsSync,
  readdirSync,
  readFileSync,
  statSync,
  writeFileSync,
} from "node:fs";
import { dirname, join } from "node:path";
import { fileURLToPath } from "node:url";

const ROOT = join(dirname(fileURLToPath(import.meta.url)), "..");
const TAURI = join(ROOT, "src-tauri");
const LICENCE_FILE = /^(licen[cs]e|copying|notice|unlicense)([-._].*)?$/i;
const COPYLEFT = /GPL|AGPL|LGPL|SSPL|BUSL|MPL|UNLICENSED|UNKNOWN|SEE LICENSE/i;
const TARGETS = ["x86_64-unknown-linux-gnu", "x86_64-pc-windows-msvc"];
// These texts carry no copyright line, so any copy of them is the same text.
const STANDARD_TEXT = ["MPL-2.0", "Apache-2.0"];

function run(command, args, cwd) {
  return execFileSync(command, args, {
    cwd,
    encoding: "utf8",
    maxBuffer: 1 << 28,
  });
}

/** Where Node finds `name` when asked from `fromDir`: the nearest node_modules that holds it. */
function resolvePackage(name, fromDir) {
  for (let dir = fromDir; ; dir = dirname(dir)) {
    const candidate = join(dir, "node_modules", name);
    if (existsSync(join(candidate, "package.json"))) return candidate;
    if (dirname(dir) === dir) return null;
  }
}

/** npm packages installed for production: the `dependencies` of the app, followed recursively. */
function npmPackages() {
  const found = new Map();
  const missing = [];
  const visit = (fromDir, dependencies) => {
    for (const name of Object.keys(dependencies ?? {})) {
      const path = resolvePackage(name, fromDir);
      if (!path) {
        missing.push(`${name} (from ${fromDir})`);
        continue;
      }
      const manifest = JSON.parse(
        readFileSync(join(path, "package.json"), "utf8"),
      );
      const key = `${manifest.name}@${manifest.version}`;
      if (found.has(key)) continue;
      found.set(key, {
        name: manifest.name,
        version: manifest.version,
        dir: path,
        license:
          typeof manifest.license === "string" ? manifest.license : "UNKNOWN",
        repository: manifest.repository,
      });
      visit(path, manifest.dependencies);
    }
  };
  visit(
    ROOT,
    JSON.parse(readFileSync(join(ROOT, "package.json"), "utf8")).dependencies,
  );
  if (missing.length > 0)
    throw new Error(`npm packages not installed: ${missing.join(", ")}`);
  return [...found.values()];
}

/** Crates the app links, on each platform it ships for; workspace crates are Lynx's own. */
function cargoCrates() {
  const wanted = new Set();
  for (const target of TARGETS) {
    const lines = run(
      "cargo",
      [
        "tree",
        "-p",
        "lynx-ide",
        "--edges",
        "normal",
        "--prefix",
        "none",
        "--target",
        target,
        "--format",
        "{p}",
      ],
      TAURI,
    ).split("\n");
    for (const line of lines) {
      const match = line.match(/^(\S+) v(\S+)/);
      if (match) wanted.add(`${match[1]}@${match[2]}`);
    }
  }
  const meta = JSON.parse(
    run("cargo", ["metadata", "--format-version", "1", "--locked"], TAURI),
  );
  const members = new Set(meta.workspace_members);
  return meta.packages
    .filter(
      (pkg) => !members.has(pkg.id) && wanted.has(`${pkg.name}@${pkg.version}`),
    )
    .map((pkg) => ({
      name: pkg.name,
      version: pkg.version,
      dir: dirname(pkg.manifest_path),
      license: pkg.license ?? "UNKNOWN",
      repository: pkg.repository,
    }));
}

/** The licence file of the package's GitHub repository, for packages that ship without one. */
function upstreamLicence(repository) {
  const url = typeof repository === "string" ? repository : repository?.url;
  const match = url?.match(
    /github\.com[/:]([^/]+)\/([^/#]+?)(?:\.git)?(?:[/#].*)?$/,
  );
  if (!match) return null;
  try {
    const content = run(
      "gh",
      ["api", `repos/${match[1]}/${match[2]}/license`, "--jq", ".content"],
      ROOT,
    );
    return Buffer.from(content.trim(), "base64").toString("utf8").trim();
  } catch {
    return null;
  }
}

function licenceTexts(dir) {
  if (!existsSync(dir)) return [];
  return readdirSync(dir)
    .filter(
      (file) => LICENCE_FILE.test(file) && statSync(join(dir, file)).isFile(),
    )
    .sort()
    .map((file) => readFileSync(join(dir, file), "utf8").trim());
}

const packages = [
  ...npmPackages().map((pkg) => ({ ...pkg, ecosystem: "npm" })),
  ...cargoCrates().map((pkg) => ({ ...pkg, ecosystem: "cargo" })),
].sort(
  (a, b) => a.name.localeCompare(b.name) || a.version.localeCompare(b.version),
);

const groups = new Map();
const withoutFile = [];
const standardTexts = new Map();
for (const pkg of packages) {
  let texts = licenceTexts(pkg.dir);
  let label = `${pkg.name} ${pkg.version} (${pkg.license})`;
  if (texts.length === 0) {
    const upstream = upstreamLicence(pkg.repository);
    const standard = STANDARD_TEXT.includes(pkg.license)
      ? standardTexts.get(pkg.license)
      : undefined;
    if (upstream) {
      texts = [upstream];
      label += ", licence text from the upstream repository";
    } else if (standard) {
      texts = [standard];
      label += ", standard licence text";
    } else {
      withoutFile.push(label);
      continue;
    }
  }
  if (
    STANDARD_TEXT.includes(pkg.license) &&
    !standardTexts.has(pkg.license) &&
    texts.length > 0
  ) {
    standardTexts.set(pkg.license, texts[0]);
  }
  const key = createHash("sha256").update(texts.join("\n----\n")).digest("hex");
  if (!groups.has(key)) groups.set(key, { texts, packages: [] });
  groups.get(key).packages.push(label);
}

const lines = [
  "# Third-party licences",
  "",
  "Generated by `bun scripts/third-party-licenses.mjs` from the npm packages and Cargo crates that go",
  "into Lynx. Do not edit it by hand. Lynx's own licence is in LICENSE and NOTICE.",
  "",
  "## Packages without a licence file",
  "",
  "Their licence is named in the package manifest, which is listed here.",
  "",
  ...withoutFile.map((label) => `- ${label}`),
  "",
];
let index = 0;
for (const group of groups.values()) {
  index += 1;
  lines.push(`## Licence text ${index}`, "", "Used by:", "");
  lines.push(
    ...group.packages.map((label) => `- ${label}`),
    "",
    "```text",
    group.texts.join("\n\n----\n\n"),
    "```",
    "",
  );
}
writeFileSync(join(ROOT, "THIRD_PARTY_LICENSES.md"), lines.join("\n"));

const copyleft = packages.filter((pkg) => COPYLEFT.test(pkg.license));
console.log(
  `${packages.length} packages (${packages.filter((p) => p.ecosystem === "npm").length} npm, ${packages.filter((p) => p.ecosystem === "cargo").length} cargo), ${groups.size} licence texts, ${withoutFile.length} without a licence file.`,
);
if (copyleft.length > 0) {
  console.log("Packages with a copyleft or unknown licence, to review:");
  for (const pkg of copyleft)
    console.log(`  ${pkg.name} ${pkg.version}: ${pkg.license}`);
}
