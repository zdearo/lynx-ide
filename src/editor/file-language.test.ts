import { describe, expect, it } from "vitest";

import { languageNameFor } from "@/editor/file-language";

describe("languageNameFor", () => {
  it("reads the language from the extension, whatever its case", () => {
    expect(languageNameFor("src/App.tsx")).toBe("tsx");
    expect(languageNameFor("lib/util.MJS")).toBe("javascript");
    expect(languageNameFor("README.md")).toBe("markdown");
    expect(languageNameFor("src-tauri/src/lib.rs")).toBe("rust");
  });

  it("is plain text for an unknown extension, no extension, or a dotfile", () => {
    expect(languageNameFor("notes.txt")).toBeNull();
    expect(languageNameFor("Makefile")).toBeNull();
    expect(languageNameFor(".gitignore")).toBeNull();
  });
});
