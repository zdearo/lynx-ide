import { HighlightStyle, syntaxHighlighting } from "@codemirror/language";
import { EditorView } from "@codemirror/view";
import { tags } from "@lezer/highlight";

/** The editor looks like the rest of the app: every colour is a token of `index.css`. */
export const editorTheme = EditorView.theme({
  "&": {
    height: "100%",
    color: "var(--foreground)",
    backgroundColor: "var(--background)",
    fontSize: "var(--text-ui)",
  },
  "&.cm-focused": { outline: "none" },
  ".cm-scroller": { fontFamily: "var(--font-mono)", lineHeight: "1.6" },
  ".cm-content": { caretColor: "var(--foreground)", padding: "0.5rem 0" },
  ".cm-cursor": { borderLeftColor: "var(--foreground)" },
  ".cm-gutters": {
    backgroundColor: "var(--background)",
    color: "var(--muted-foreground)",
    border: "none",
  },
  ".cm-activeLine": {
    backgroundColor: "color-mix(in oklab, var(--foreground) 5%, transparent)",
  },
  ".cm-activeLineGutter": {
    backgroundColor: "transparent",
    color: "var(--foreground)",
  },
  ".cm-selectionBackground, &.cm-focused .cm-selectionBackground": {
    backgroundColor: "color-mix(in oklab, var(--foreground) 22%, transparent)",
  },
  ".cm-searchMatch": {
    backgroundColor: "color-mix(in oklab, var(--attention) 35%, transparent)",
  },
  ".cm-panels": {
    backgroundColor: "var(--sidebar)",
    color: "var(--foreground)",
    borderTop: "1px solid var(--sidebar-border)",
  },
  ".cm-panels input, .cm-panels button": {
    color: "var(--foreground)",
    backgroundColor: "var(--background)",
    border: "1px solid var(--border)",
  },
});

const highlightStyle = HighlightStyle.define([
  {
    tag: [tags.keyword, tags.modifier, tags.controlKeyword],
    color: "var(--code-keyword)",
  },
  {
    tag: [tags.string, tags.regexp, tags.special(tags.string)],
    color: "var(--code-string)",
  },
  {
    tag: [tags.number, tags.bool, tags.null, tags.atom],
    color: "var(--code-number)",
  },
  { tag: tags.comment, color: "var(--code-comment)", fontStyle: "italic" },
  {
    tag: [
      tags.function(tags.variableName),
      tags.function(tags.propertyName),
      tags.definition(tags.function(tags.variableName)),
    ],
    color: "var(--code-function)",
  },
  {
    tag: [tags.typeName, tags.className, tags.namespace],
    color: "var(--code-type)",
  },
  {
    tag: [tags.propertyName, tags.tagName, tags.attributeName],
    color: "var(--code-property)",
  },
  { tag: tags.heading, color: "var(--code-keyword)", fontWeight: "bold" },
  { tag: tags.strong, fontWeight: "bold" },
  { tag: tags.emphasis, fontStyle: "italic" },
  { tag: tags.link, textDecoration: "underline" },
]);

export const editorHighlight = syntaxHighlighting(highlightStyle);

const added = (percent: number) =>
  `color-mix(in oklab, var(--diff-added) ${percent}%, transparent)`;
const removed = (percent: number) =>
  `color-mix(in oklab, var(--diff-removed) ${percent}%, transparent)`;

/**
 * Colours for the diff view; the merge package ships its own, which this replaces. Its base
 * theme mounts first, so a rule with the same selector shape here wins over its rule.
 */
export const diffTheme = EditorView.theme({
  "&": { height: "100%" },
  // Lines: the original side and deleted chunks show removals, the new side shows additions.
  "&.cm-merge-a .cm-changedLine, & .cm-deletedChunk": {
    backgroundColor: removed(12),
  },
  "&.cm-merge-b .cm-changedLine, & .cm-inlineChangedLine, & .cm-insertedLine": {
    backgroundColor: added(12),
  },
  "&.cm-merge-a .cm-changedText": {
    background: "none",
    backgroundColor: removed(16),
  },
  "&.cm-merge-b .cm-changedText": {
    background: "none",
    backgroundColor: added(16),
  },
  "& .cm-deletedChunk .cm-deletedText, &.cm-merge-b .cm-deletedText": {
    background: "none",
    backgroundColor: removed(16),
  },
  ".cm-insertedLine, .cm-deletedLine, .cm-deletedLine del": {
    textDecoration: "none",
  },
  ".cm-deletedChunk": { paddingLeft: "6px" },
  // The bar beside each changed line. The gutter column itself stays clear.
  "&.cm-merge-a .cm-changedLineGutter": { background: "var(--diff-removed)" },
  "&.cm-merge-b .cm-changedLineGutter": { background: "var(--diff-added)" },
  ".cm-inlineChangedLineGutter": { background: "var(--diff-added)" },
  "& .cm-deletedLineGutter": { background: "var(--diff-removed)" },
  // Unchanged runs collapse into a thin bar that opens them on click.
  "& .cm-collapsedLines": {
    padding: "0.125rem 0.75rem",
    color: "var(--muted-foreground)",
    background: "var(--sidebar)",
    borderTop: "1px solid var(--border)",
    borderBottom: "1px solid var(--border)",
    fontSize: "var(--text-meta)",
    lineHeight: "1rem",
    cursor: "pointer",
  },
  ".cm-collapsedLines:before, .cm-collapsedLines:after": { content: "none" },
  // Where one side has no line to match the other: a flat block, not stripes (those read as noise).
  ".cm-mergeSpacer": { background: "var(--sidebar)" },
  ".cm-mergeView": { height: "100%" },
  ".cm-mergeViewEditors": { height: "100%" },
  ".cm-merge-a, .cm-merge-b": { height: "100%", overflow: "auto" },
  ".cm-merge-b": { borderLeft: "1px solid var(--border)" },
});
