import { createElement } from "react";
import { describe, expect, it } from "vitest";

import { languageOf, textOf } from "@/bots/code-text";

describe("textOf", () => {
  it("joins the text of nested elements and drops the markup", () => {
    const node = createElement(
      "code",
      null,
      "minha-cli ",
      createElement("span", { className: "tok" }, "serve"),
      ["--port ", 8080],
    );

    expect(textOf(node)).toBe("minha-cli serve--port 8080");
  });

  it("is empty for nothing", () => {
    expect(textOf(null)).toBe("");
    expect(textOf(false)).toBe("");
  });
});

describe("languageOf", () => {
  it("reads the language a fenced block names", () => {
    expect(
      languageOf(createElement("code", { className: "language-bash" })),
    ).toBe("bash");
    expect(
      languageOf(createElement("code", { className: "hljs language-json x" })),
    ).toBe("json");
  });

  it("is null for a block with no language", () => {
    expect(languageOf(createElement("code", null, "x"))).toBeNull();
    expect(languageOf("texto")).toBeNull();
  });
});
