import { describe, expect, it } from "vitest";

import { lspSummary } from "@/status-bar/lsp-summary";

describe("lspSummary", () => {
  it("says no server runs when none does", () => {
    expect(lspSummary([])).toBe("No language server running");
  });

  it("names each running server with its state", () => {
    expect(
      lspSummary([
        { language: "typescript", state: "ready" },
        { language: "rust", state: "indexing" },
      ]),
    ).toBe("Language servers: typescript (ready), rust (indexing)");
  });
});
