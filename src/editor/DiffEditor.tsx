import { useEffect, useRef } from "react";
import { Compartment, EditorState, type Extension } from "@codemirror/state";
import { EditorView, lineNumbers } from "@codemirror/view";
import { bracketMatching } from "@codemirror/language";
import { goToNextChunk, MergeView, unifiedMergeView } from "@codemirror/merge";

import type { DiffMode } from "@/editor/diff-mode";
import { diffTheme, editorHighlight, editorTheme } from "@/editor/editor-theme";
import { languageNameFor, loadLanguage } from "@/editor/file-language";
import { copy } from "@/ui-copy";

const language = new Compartment();

const COLLAPSE = { margin: 3, minSize: 6 };

function viewerExtensions(path: string): Extension[] {
  return [
    lineNumbers(),
    EditorState.phrases.of({ "$ unchanged lines": copy.diff.unchanged }),
    bracketMatching(),
    EditorState.readOnly.of(true),
    EditorView.editable.of(false),
    EditorView.contentAttributes.of({
      "aria-label": copy.diff.label(path.split("/").pop() ?? path),
      "aria-readonly": "true",
    }),
    editorTheme,
    diffTheme,
    editorHighlight,
    language.of([]),
  ];
}

interface DiffEditorProps {
  path: string;
  original: string;
  modified: string;
  mode: DiffMode;
}

/** Read-only CodeMirror diff of two texts. Rebuilt whenever a text or the mode changes. */
export function DiffEditor({
  path,
  original,
  modified,
  mode,
}: DiffEditorProps) {
  const host = useRef<HTMLDivElement>(null);

  useEffect(() => {
    const parent = host.current;
    if (!parent) return;
    const extensions = viewerExtensions(path);
    let views: EditorView[];
    let destroy: () => void;

    if (mode === "split") {
      const merge = new MergeView({
        a: { doc: original, extensions },
        b: { doc: modified, extensions },
        parent,
        gutter: true,
        highlightChanges: true,
        collapseUnchanged: COLLAPSE,
      });
      views = [merge.a, merge.b];
      destroy = () => merge.destroy();
      goToNextChunk(merge.b);
    } else {
      const view = new EditorView({
        parent,
        doc: modified,
        extensions: [
          ...extensions,
          unifiedMergeView({
            original,
            mergeControls: false,
            gutter: true,
            highlightChanges: true,
            collapseUnchanged: COLLAPSE,
          }),
        ],
      });
      views = [view];
      destroy = () => view.destroy();
      goToNextChunk(view);
    }

    let disposed = false;
    const name = languageNameFor(path);
    if (name) {
      void loadLanguage(name).then((loaded) => {
        if (disposed) return;
        for (const view of views) {
          view.dispatch({ effects: language.reconfigure(loaded) });
        }
      });
    }

    return () => {
      disposed = true;
      destroy();
    };
  }, [path, original, modified, mode]);

  return <div ref={host} className="h-full min-h-0" data-diff={path} />;
}
