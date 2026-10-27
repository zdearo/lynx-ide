import { isValidElement, type ReactNode } from "react";

/** The text a node shows, without its markup: what a person would select and copy. */
export function textOf(node: ReactNode): string {
  if (typeof node === "string" || typeof node === "number") return String(node);
  if (Array.isArray(node)) return node.map(textOf).join("");
  if (isValidElement<{ children?: ReactNode }>(node)) {
    return textOf(node.props.children);
  }
  return "";
}

/** The language a fenced block names (`bash` for ```bash), from the class the Markdown gives its code. */
export function languageOf(node: ReactNode): string | null {
  if (!isValidElement<{ className?: string }>(node)) return null;
  const match = /(?:^|\s)language-(\S+)/.exec(node.props.className ?? "");
  return match?.[1] ?? null;
}
