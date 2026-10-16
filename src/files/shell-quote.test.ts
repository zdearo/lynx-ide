import { describe, expect, it } from "vitest";

import { quotePathForShell } from "@/files/shell-quote";

describe("quotePathForShell (POSIX)", () => {
  it("leaves plain paths alone", () => {
    expect(quotePathForShell("src/main.rs")).toBe("src/main.rs");
    expect(quotePathForShell("a-b_c.d/e+f@g")).toBe("a-b_c.d/e+f@g");
  });

  it("quotes spaces and shell characters", () => {
    expect(quotePathForShell("my docs/file.md")).toBe("'my docs/file.md'");
    expect(quotePathForShell("a;b")).toBe("'a;b'");
    expect(quotePathForShell("$HOME/x")).toBe("'$HOME/x'");
    expect(quotePathForShell("a&b|c")).toBe("'a&b|c'");
    expect(quotePathForShell("(x)")).toBe("'(x)'");
  });

  it("escapes a single quote inside the path", () => {
    expect(quotePathForShell("it's here")).toBe("'it'\\''s here'");
  });

  it("quotes a leading dash-free glob and unicode safely", () => {
    expect(quotePathForShell("*.log")).toBe("'*.log'");
    expect(quotePathForShell("café/ação.md")).toBe("'café/ação.md'");
  });
});

describe("quotePathForShell (Windows)", () => {
  it("leaves plain paths alone and double-quotes the rest", () => {
    expect(quotePathForShell("src\\main.rs", true)).toBe("src\\main.rs");
    expect(quotePathForShell("my docs\\f.md", true)).toBe('"my docs\\f.md"');
  });
});
