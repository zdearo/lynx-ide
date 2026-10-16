import { describe, expect, it } from "vitest";

import { fileIconKind, type FileIconKind } from "@/files/file-icon-kind";

const file = (name: string) => fileIconKind(name, false, false);

describe("folders", () => {
  it("picks plain folder icons by open state", () => {
    expect(fileIconKind("src", true, false)).toBe("folder");
    expect(fileIconKind("src", true, true)).toBe("folder-open");
  });

  it("gives tooling folders their own icon, open or not, whatever the case", () => {
    expect(fileIconKind(".github", true, false)).toBe("folder-git");
    expect(fileIconKind(".GitHub", true, true)).toBe("folder-git");
    expect(fileIconKind(".vscode", true, false)).toBe("folder-config");
    expect(fileIconKind(".husky", true, true)).toBe("folder-config");
  });

  it("does not apply file rules to folders", () => {
    expect(fileIconKind("package.json", true, false)).toBe("folder");
  });
});

describe("exact names beat extensions", () => {
  const cases: Array<[string, FileIconKind]> = [
    ["package.json", "package"],
    ["Cargo.toml", "package"],
    ["pyproject.toml", "package"],
    ["Gemfile", "package"],
    ["package-lock.json", "lockfile"],
    ["bun.lock", "lockfile"],
    ["Cargo.lock", "lockfile"],
    ["Dockerfile", "docker"],
    [".dockerignore", "docker"],
    ["compose.yaml", "docker"],
    ["Makefile", "build"],
    ["CMakeLists.txt", "build"],
    ["justfile", "build"],
    [".gitignore", "git"],
    [".gitattributes", "git"],
    [".env", "secret"],
    ["LICENSE", "license"],
    ["COPYING", "license"],
    ["README", "readme"],
  ];
  it.each(cases)("%s is %s", (name, kind) => {
    expect(file(name)).toBe(kind);
  });
});

describe("prefixes and tooling configuration", () => {
  const cases: Array<[string, FileIconKind]> = [
    ["README.md", "readme"],
    ["readme.rst", "readme"],
    ["LICENSE-MIT", "license"],
    ["LICENSE.txt", "license"],
    ["Dockerfile.dev", "docker"],
    ["docker-compose.override.yml", "docker"],
    [".env.local", "secret"],
    [".env.production", "secret"],
    ["tsconfig.json", "config"],
    ["tsconfig.node.json", "config"],
    [".eslintrc.cjs", "config"],
    [".prettierrc.json", "config"],
    ["vite.config.ts", "config"],
    ["jest.config.js", "config"],
    ["CHANGELOG.md", "text"],
  ];
  it.each(cases)("%s is %s", (name, kind) => {
    expect(file(name)).toBe(kind);
  });
});

describe("extensions", () => {
  const cases: Array<[string, FileIconKind]> = [
    ["main.rs", "code"],
    ["App.TSX", "code"],
    ["styles.scss", "code"],
    ["deploy.sh", "terminal"],
    ["data.json", "data"],
    ["settings.toml", "config"],
    ["notes.md", "text"],
    ["report.pdf", "text"],
    ["table.csv", "spreadsheet"],
    ["logo.png", "image"],
    ["backup.tar", "archive"],
    ["bundle.zip", "archive"],
    ["song.mp3", "audio"],
    ["clip.mp4", "video"],
    ["Inter.woff2", "font"],
    ["app.sqlite", "database"],
    ["tool.exe", "binary"],
    ["module.wasm", "binary"],
    ["server.pem", "secret"],
    ["yarn.lock", "lockfile"],
    ["fix.patch", "git"],
  ];
  it.each(cases)("%s is %s", (name, kind) => {
    expect(file(name)).toBe(kind);
  });
});

describe("fallbacks", () => {
  it("treats an unknown name or extension as a plain file", () => {
    expect(file("Procfile")).toBe("file");
    expect(file("notes.unknownext")).toBe("file");
    expect(file("archive.")).toBe("file");
  });

  it("treats other dotfiles as a tool's settings", () => {
    expect(file(".prettierignore")).toBe("config");
    expect(file(".bashrc")).toBe("config");
  });

  it("uses the last extension of a multi-dot name", () => {
    expect(file("archive.tar.gz")).toBe("archive");
    expect(file("component.test.tsx")).toBe("code");
  });
});
