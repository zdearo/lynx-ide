import { createCn } from "cn/config";

/**
 * `cn` also has to know the font sizes defined in index.css (text-ui, text-nested,
 * text-meta). Without this it reads them as colours and drops them whenever a text
 * colour follows, so the affected labels silently fall back to the body size.
 */
export const cn = createCn({
  extend: {
    classGroups: { "font-size": [{ text: ["ui", "nested", "meta"] }] },
  },
});
