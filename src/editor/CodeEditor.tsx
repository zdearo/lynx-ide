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
  autosaveSlot,
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
  /** Called after a pause in typing, when the view loses focus, and when the view goes away. */
  onAutosave: (text: string) => void;
}

const AUTOSAVE_DELAY_MS = 1000;

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
  onAutosave,
}: CodeEditorProps) {
  const host = useRef<HTMLDivElement>(null);
  // The callbacks change on every render; the view is built once and reads them from here.
  const handlers = useRef({ onChange, onSave, onAutosave });
  useEffect(() => {
    handlers.current = { onChange, onSave, onAutosave };
  });

  useEffect(() => {
    const parent = host.current;
    if (!parent) return;

    const slot = autosaveSlot(documentId);
    const cancelAutosave = () => {
      clearTimeout(slot.timer);
      slot.timer = undefined;
    };
    // Writes now if a write is waiting for the pause to end.
    const flush = () => {
      if (slot.timer === undefined || !slot.view) return;
      cancelAutosave();
      slot.save(slot.view.state.doc.toString());
    };

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
                cancelAutosave();
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
            if (!update.docChanged) return;
            handlers.current.onChange();
            clearTimeout(slot.timer);
            slot.timer = setTimeout(() => {
              slot.timer = undefined;
              slot.save(update.state.doc.toString());
            }, AUTOSAVE_DELAY_MS);
          }),
          EditorView.domEventHandlers({
            blur: () => {
              flush();
            },
          }),
        ],
      });

    const current = new EditorView({ state, parent });
    slot.view = current;
    slot.save = (text) => handlers.current.onAutosave(text);
    if (kept?.scroll) current.dispatch({ effects: kept.scroll });
    setLiveText(documentId, () => current.state.doc.toString());
    current.focus();

    let disposed = false;
    const name = languageNameFor(path);
    if (name) {
      void loadLanguage(name).then((extension) => {
        if (!disposed)
          current.dispatch({ effects: language.reconfigure(extension) });
      });
    }

    return () => {
      disposed = true;
      flush();
      slot.view = undefined;
      setLiveText(documentId, null);
      setDocument(documentId, {
        state: current.state,
        scroll: current.scrollSnapshot(),
        revision,
      });
      current.destroy();
    };
    // The parent remounts this for a reload (new `key`), so only the file matters here.
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [documentId, path, revision]);

  return <div ref={host} className="h-full min-h-0" data-editor={path} />;
}
