import type { Extension } from "@codemirror/state";

export type LanguageName =
  | "javascript"
  | "typescript"
  | "jsx"
  | "tsx"
  | "json"
  | "css"
  | "html"
  | "markdown"
  | "php"
  | "python"
  | "rust";

const BY_EXTENSION: Record<string, LanguageName> = {
  js: "javascript",
  mjs: "javascript",
  cjs: "javascript",
  jsx: "jsx",
  ts: "typescript",
  mts: "typescript",
  cts: "typescript",
  tsx: "tsx",
  json: "json",
  css: "css",
  html: "html",
  htm: "html",
  md: "markdown",
  markdown: "markdown",
  php: "php",
  py: "python",
  rs: "rust",
};

/** The language of a file by its extension, or null for plain text. */
export function languageNameFor(path: string): LanguageName | null {
  const name = path.split("/").pop() ?? path;
  const dot = name.lastIndexOf(".");
  if (dot <= 0) return null;
  return BY_EXTENSION[name.slice(dot + 1).toLowerCase()] ?? null;
}

/** Each language loads on its own, the first time a file of that language opens. */
export async function loadLanguage(name: LanguageName): Promise<Extension> {
  switch (name) {
    case "javascript":
    case "jsx":
    case "typescript":
    case "tsx": {
      const { javascript } = await import("@codemirror/lang-javascript");
      return javascript({
        jsx: name === "jsx" || name === "tsx",
        typescript: name === "typescript" || name === "tsx",
      });
    }
    case "json":
      return (await import("@codemirror/lang-json")).json();
    case "css":
      return (await import("@codemirror/lang-css")).css();
    case "html":
      return (await import("@codemirror/lang-html")).html();
    case "markdown":
      return (await import("@codemirror/lang-markdown")).markdown();
    case "php":
      return (await import("@codemirror/lang-php")).php();
    case "python":
      return (await import("@codemirror/lang-python")).python();
    case "rust":
      return (await import("@codemirror/lang-rust")).rust();
  }
}
