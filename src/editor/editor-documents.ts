import type { EditorState, StateEffect } from "@codemirror/state";

/** What an open file keeps while another one is on screen: its editor state, undo history included, and its scroll. */
export interface DocumentSnapshot {
  state: EditorState;
  scroll: StateEffect<unknown> | null;
  /** The kept state belongs to one load of the file; a reload makes it stale. */
  revision: number;
}

const documents = new Map<string, DocumentSnapshot>();

export function documentKey(worktreePath: string, path: string): string {
  return `${worktreePath}\u0000${path}`;
}

export function getDocument(key: string): DocumentSnapshot | undefined {
  return documents.get(key);
}

export function setDocument(key: string, snapshot: DocumentSnapshot): void {
  documents.set(key, snapshot);
}

export function forgetDocument(key: string): void {
  documents.delete(key);
}

const liveViews = new Map<string, () => string>();

/** The text on screen for a file, for actions that start outside the editor. */
export function liveText(key: string): string | undefined {
  return liveViews.get(key)?.();
}

export function setLiveText(key: string, read: (() => string) | null): void {
  if (read) liveViews.set(key, read);
  else liveViews.delete(key);
}
