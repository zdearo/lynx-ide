import { useEffect, useRef } from "react";
import {
  defaultKeymap,
  history,
  historyKeymap,
  indentWithTab,
} from "@codemirror/commands";
import { bracketMatching, indentOnInput } from "@codemirror/language";
import { highlightSelectionMatches, searchKeymap } from "@codemirror/search";
import { Compartment, EditorState } from "@codemirror/state";
import {
  drawSelection,
  EditorView,
  highlightActiveLine,
  highlightActiveLineGutter,
  keymap,
  lineNumbers,
} from "@codemirror/view";

import {
  getDocument,
  setDocument,
  setLiveText,
} from "@/editor/editor-documents";
import { editorHighlight, editorTheme } from "@/editor/editor-theme";
import { copy } from "@/ui-copy";
import { languageNameFor, loadLanguage } from "@/editor/file-language";

interface CodeEditorProps {
  /** Where this file's state is kept while another one is on screen. */
  documentId: string;
  path: string;
  revision: number;
  /** The text to start from when the file has no kept state. */
  text: string;
  onChange: () => void;
  onSave: (text: string) => void;
}

// One compartment for every file: a kept state still finds its language slot.
const language = new Compartment();

/** One CodeMirror view for one file. Its state outlives it in `editor-documents`. */
export function CodeEditor({
  documentId,
  path,
  revision,
  text,
  onChange,
  onSave,
}: CodeEditorProps) {
  const host = useRef<HTMLDivElement>(null);
  // The callbacks change on every render; the view is built once and reads them from here.
  const handlers = useRef({ onChange, onSave });
  useEffect(() => {
    handlers.current = { onChange, onSave };
  });

  useEffect(() => {
    const parent = host.current;
    if (!parent) return;

    const found = getDocument(documentId);
    const kept = found?.revision === revision ? found : undefined;
    const state =
      kept?.state ??
      EditorState.create({
        doc: text,
        extensions: [
          lineNumbers(),
          highlightActiveLineGutter(),
          highlightActiveLine(),
          history(),
          drawSelection(),
          indentOnInput(),
          bracketMatching(),
          highlightSelectionMatches(),
          EditorView.contentAttributes.of({
            "aria-label": copy.editor.label(path.split("/").pop() ?? path),
          }),
          editorTheme,
          editorHighlight,
          language.of([]),
          keymap.of([
            {
              key: "Mod-s",
              preventDefault: true,
              run: (view) => {
                handlers.current.onSave(view.state.doc.toString());
                return true;
              },
            },
            ...defaultKeymap,
            ...historyKeymap,
            ...searchKeymap,
            indentWithTab,
          ]),
          EditorView.updateListener.of((update) => {
            if (update.docChanged) handlers.current.onChange();
          }),
        ],
      });

    const view = new EditorView({ state, parent });
    if (kept?.scroll) view.dispatch({ effects: kept.scroll });
    setLiveText(documentId, () => view.state.doc.toString());
    view.focus();

    let disposed = false;
    const name = languageNameFor(path);
    if (name) {
      void loadLanguage(name).then((extension) => {
        if (!disposed)
          view.dispatch({ effects: language.reconfigure(extension) });
      });
    }

    return () => {
      disposed = true;
      setLiveText(documentId, null);
      setDocument(documentId, {
        state: view.state,
        scroll: view.scrollSnapshot(),
        revision,
      });
      view.destroy();
    };
    // The parent remounts this for a reload (new `key`), so only the file matters here.
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [documentId, path, revision]);

  return <div ref={host} className="h-full min-h-0" data-editor={path} />;
}
