export type FileIconKind =
  | "folder"
  | "folder-open"
  | "folder-git"
  | "folder-config"
  | "code"
  | "terminal"
  | "data"
  | "config"
  | "text"
  | "readme"
  | "image"
  | "archive"
  | "audio"
  | "video"
  | "font"
  | "spreadsheet"
  | "database"
  | "binary"
  | "package"
  | "docker"
  | "build"
  | "license"
  | "git"
  | "secret"
  | "lockfile"
  | "file";

/** Exact names, lowercase, checked before anything else. */
const BY_NAME: Record<string, FileIconKind> = {
  "package.json": "package",
  "cargo.toml": "package",
  "pyproject.toml": "package",
  "go.mod": "package",
  gemfile: "package",
  "composer.json": "package",
  "requirements.txt": "package",
  "pom.xml": "package",
  "build.gradle": "package",
  "build.gradle.kts": "package",
  "deno.json": "package",

  "package-lock.json": "lockfile",
  "pnpm-lock.yaml": "lockfile",
  "yarn.lock": "lockfile",
  "bun.lockb": "lockfile",
  "bun.lock": "lockfile",
  "cargo.lock": "lockfile",
  "poetry.lock": "lockfile",
  "composer.lock": "lockfile",
  "gemfile.lock": "lockfile",
  "go.sum": "lockfile",

  dockerfile: "docker",
  ".dockerignore": "docker",
  "compose.yml": "docker",
  "compose.yaml": "docker",

  makefile: "build",
  "cmakelists.txt": "build",
  justfile: "build",
  rakefile: "build",
  "taskfile.yml": "build",
  "gulpfile.js": "build",
  "gruntfile.js": "build",

  ".gitignore": "git",
  ".gitattributes": "git",
  ".gitmodules": "git",
  ".gitkeep": "git",
  ".mailmap": "git",

  ".env": "secret",
  id_rsa: "secret",
  id_ed25519: "secret",
  ".npmrc": "config",

  license: "license",
  licence: "license",
  copying: "license",
  unlicense: "license",
  readme: "readme",
};

/** Names that start the same way, lowercase, checked after the exact names. */
const BY_PREFIX: ReadonlyArray<readonly [string, FileIconKind]> = [
  ["readme", "readme"],
  ["license", "license"],
  ["licence", "license"],
  ["dockerfile", "docker"],
  ["docker-compose", "docker"],
  [".env.", "secret"],
  ["tsconfig", "config"],
  ["jsconfig", "config"],
  [".eslintrc", "config"],
  [".prettierrc", "config"],
  [".babelrc", "config"],
  [".editorconfig", "config"],
  ["changelog", "text"],
  ["contributing", "text"],
];

function extensionKinds(): Record<string, FileIconKind> {
  const groups: ReadonlyArray<readonly [FileIconKind, string]> = [
    [
      "code",
      "ts tsx js jsx mjs cjs mts cts rs py go java kt kts c h cpp cc hpp cs rb php swift lua dart scala ex exs erl hs ml clj zig nim r jl pl graphql gql proto css scss sass less html htm xml vue svelte astro sql",
    ],
    ["terminal", "sh bash zsh fish ps1 bat cmd"],
    ["data", "json jsonc json5 jsonl ndjson"],
    ["config", "toml yaml yml ini conf cfg properties plist editorconfig"],
    ["text", "md mdx txt rst log adoc org tex pdf doc docx odt rtf"],
    ["spreadsheet", "csv tsv xls xlsx ods"],
    ["image", "png jpg jpeg gif webp svg ico avif bmp tiff tif heic psd"],
    ["archive", "zip tar gz tgz bz2 xz 7z rar zst jar deb rpm"],
    ["audio", "mp3 wav flac ogg m4a aac"],
    ["video", "mp4 mov mkv webm avi"],
    ["font", "ttf otf woff woff2 eot"],
    ["database", "sqlite sqlite3 db"],
    ["binary", "exe dll so dylib bin o wasm"],
    ["lockfile", "lock"],
    ["secret", "pem key crt cer"],
    ["git", "patch diff"],
  ];
  const map: Record<string, FileIconKind> = {};
  for (const [kind, extensions] of groups) {
    for (const extension of extensions.split(" ")) map[extension] = kind;
  }
  return map;
}

const BY_EXTENSION = extensionKinds();

/** Folders with a purpose get their own icon; the rest stay plain. */
const SPECIAL_FOLDERS: Record<string, FileIconKind> = {
  ".github": "folder-git",
  ".gitlab": "folder-git",
  ".git": "folder-git",
  ".vscode": "folder-config",
  ".idea": "folder-config",
  ".config": "folder-config",
  ".husky": "folder-config",
  ".devcontainer": "folder-config",
};

/**
 * What kind of icon a row gets. Names win over extensions (`package.json` is a
 * package, not just JSON), and anything unknown stays a plain file.
 */
export function fileIconKind(
  name: string,
  isDirectory: boolean,
  open: boolean,
): FileIconKind {
  const lower = name.toLowerCase();

  if (isDirectory)
    return SPECIAL_FOLDERS[lower] ?? (open ? "folder-open" : "folder");

  const exact = BY_NAME[lower];
  if (exact) return exact;
  const prefixed = BY_PREFIX.find(([prefix]) => lower.startsWith(prefix));
  if (prefixed) return prefixed[1];
  // `vite.config.ts`, `jest.config.js`: tooling configuration, whatever the language.
  if (/\.config\.[a-z]+$/.test(lower)) return "config";

  const dot = lower.lastIndexOf(".");
  if (dot > 0) {
    const kind = BY_EXTENSION[lower.slice(dot + 1)];
    if (kind) return kind;
  }
  // A leading dot with nothing known after it (`.prettierignore`) is a tool's settings file.
  if (dot === 0) return "config";
  return "file";
}
