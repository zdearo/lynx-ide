import { create } from "zustand";

import { commands, type DiffSource, type IpcErrorKind } from "@/bindings";
import { documentKey, forgetDocument } from "@/editor/editor-documents";
import { isImagePath } from "@/editor/image-file";
import { errorMessage } from "@/lib/error-message";
import { IpcRequestError, unwrap } from "@/lib/ipc-result";
import { useTerminalStore } from "@/terminal/terminal-store";

/** What a file's tab shows: its text to edit, or its changes against HEAD. */
export type FileView = "edit" | "diff";

export interface OpenFile {
  /** Relative to the worktree, with `/`. */
  path: string;
  name: string;
  /** A preview tab is temporary: opening another preview replaces it, and editing makes it permanent. */
  preview: boolean;
  view: FileView;
  /** Where a renamed file was in HEAD, for its diff. */
  originalPath: string | null;
  /** What the diff compares: the working tree with HEAD, with the index, with the base branch. */
  diffSource: DiffSource;
  /** Set for a file of an old commit: its real path and the commit. Such a tab only shows a diff. */
  commit: { hash: string; path: string } | null;
  status: "loading" | "ready" | "error";
  /** An image file is shown, not edited: its bytes as a data URL. */
  imageUrl: string | null;
  /** The text the editor starts from. The editor itself holds what is typed after. */
  text: string;
  /** The hash of what is on disk, for the next save to notice a change. */
  hash: string | null;
  dirty: boolean;
  saving: boolean;
  /** A save found the file changed on disk. */
  conflict: boolean;
  /** Bumped when the text is replaced, so the editor starts over from it. */
  revision: number;
  error: string | null;
  errorKind: IpcErrorKind | null;
}

type SaveOutcome = "saved" | "conflict";

export interface OpenOptions {
  /** Open as a temporary tab. Opening a file that is already open as permanent keeps it permanent. */
  preview?: boolean;
  view?: FileView;
  originalPath?: string | null;
  /** Opens a file of an old commit, as a diff against its parent. `path` is then the tab's id. */
  commit?: { hash: string; path: string };
}

interface EditorStoreState {
  /** The open files of each worktree, in the order they were opened. */
  files: Record<string, OpenFile[]>;
  /** The file on screen in each worktree; null means its terminals are. */
  active: Record<string, string | null>;
  open: (
    worktreePath: string,
    path: string,
    options?: OpenOptions,
  ) => Promise<void>;
  /** Turns a preview tab into a permanent one. */
  pin: (worktreePath: string, path: string) => void;
  setView: (worktreePath: string, path: string, view: FileView) => void;
  setDiffSource: (
    worktreePath: string,
    path: string,
    source: DiffSource,
  ) => void;
  /** Shows a file, or with null the terminals again. */
  show: (worktreePath: string, path: string | null) => void;
  markDirty: (worktreePath: string, path: string) => void;
  save: (
    worktreePath: string,
    path: string,
    text: string,
    force?: boolean,
  ) => Promise<SaveOutcome>;
  reload: (worktreePath: string, path: string) => Promise<void>;
  close: (worktreePath: string, path: string) => void;
}

const fileName = (path: string) => path.split("/").pop() ?? path;

export const useEditorStore = create<EditorStoreState>((set, get) => {
  function patch(
    worktreePath: string,
    path: string,
    change: Partial<OpenFile>,
  ) {
    set((state) => ({
      files: {
        ...state.files,
        [worktreePath]: (state.files[worktreePath] ?? []).map((file) =>
          file.path === path ? { ...file, ...change } : file,
        ),
      },
    }));
  }

  function find(worktreePath: string, path: string): OpenFile | undefined {
    return get().files[worktreePath]?.find((file) => file.path === path);
  }

  async function load(worktreePath: string, path: string) {
    try {
      if (isImagePath(path)) {
        const image = await unwrap(commands.readFileImage(worktreePath, path));
        patch(worktreePath, path, {
          status: "ready",
          imageUrl: `data:${image.mime};base64,${image.data}`,
          error: null,
          errorKind: null,
          revision: (find(worktreePath, path)?.revision ?? 0) + 1,
        });
        return;
      }
      const read = await unwrap(commands.readFile(worktreePath, path));
      patch(worktreePath, path, {
        status: "ready",
        text: read.content,
        hash: read.hash,
        dirty: false,
        error: null,
        errorKind: null,
        revision: (find(worktreePath, path)?.revision ?? 0) + 1,
      });
    } catch (error) {
      patch(worktreePath, path, {
        status: "error",
        error: errorMessage(error),
        errorKind: error instanceof IpcRequestError ? error.kind : null,
      });
    }
  }

  return {
    files: {},
    active: {},

    async open(worktreePath, path, options = {}) {
      const preview = options.preview ?? false;
      const existing = find(worktreePath, path);
      if (existing) {
        patch(worktreePath, path, {
          ...(!preview && existing.preview ? { preview: false } : {}),
          ...(options.view && !isImagePath(path) ? { view: options.view } : {}),
          ...(options.originalPath !== undefined
            ? { originalPath: options.originalPath }
            : {}),
        });
        get().show(worktreePath, path);
        return;
      }
      const entry: OpenFile = {
        path,
        preview,
        view: options.commit
          ? "diff"
          : isImagePath(path)
            ? "edit"
            : (options.view ?? "edit"),
        originalPath: options.originalPath ?? null,
        diffSource: { kind: "head" },
        commit: options.commit ?? null,
        name: options.commit
          ? `${fileName(options.commit.path)} @ ${options.commit.hash.slice(0, 7)}`
          : fileName(path),
        status: options.commit ? "ready" : "loading",
        imageUrl: null,
        text: "",
        hash: null,
        dirty: false,
        saving: false,
        conflict: false,
        revision: 0,
        error: null,
        errorKind: null,
      };
      set((state) => {
        const files = state.files[worktreePath] ?? [];
        // A new preview takes the place of the old one, as in VS Code.
        const replaced = preview ? files.findIndex((file) => file.preview) : -1;
        const next =
          replaced >= 0
            ? files.map((file, index) => (index === replaced ? entry : file))
            : [...files, entry];
        if (replaced >= 0)
          forgetDocument(documentKey(worktreePath, files[replaced].path));
        return {
          files: { ...state.files, [worktreePath]: next },
          active: { ...state.active, [worktreePath]: path },
        };
      });
      // A file of an old commit has no text to read: only its diff is shown.
      if (!options.commit) await load(worktreePath, path);
    },

    pin(worktreePath, path) {
      if (find(worktreePath, path)?.preview) {
        patch(worktreePath, path, { preview: false });
      }
    },

    setView(worktreePath, path, view) {
      patch(worktreePath, path, { view });
    },

    setDiffSource(worktreePath, path, diffSource) {
      patch(worktreePath, path, { diffSource });
    },

    show(worktreePath, path) {
      set((state) => ({ active: { ...state.active, [worktreePath]: path } }));
    },

    markDirty(worktreePath, path) {
      const file = find(worktreePath, path);
      // Typing in a preview tab is a decision to keep it.
      if (file && (!file.dirty || file.preview)) {
        patch(worktreePath, path, { dirty: true, preview: false });
      }
    },

    async save(worktreePath, path, text, force = false) {
      const file = find(worktreePath, path);
      if (!file) return "saved";
      patch(worktreePath, path, { saving: true });
      try {
        const saved = await unwrap(
          commands.writeFile(
            worktreePath,
            path,
            text,
            force ? null : file.hash,
          ),
        );
        patch(worktreePath, path, {
          hash: saved.hash,
          dirty: false,
          saving: false,
          conflict: false,
        });
        return "saved";
      } catch (error) {
        if (error instanceof IpcRequestError && error.kind === "fileChanged") {
          patch(worktreePath, path, { saving: false, conflict: true });
          return "conflict";
        }
        patch(worktreePath, path, { saving: false });
        throw error;
      }
    },

    async reload(worktreePath, path) {
      forgetDocument(documentKey(worktreePath, path));
      // Nothing typed is kept, so no pending autosave may write it back over the reload.
      patch(worktreePath, path, { conflict: false, dirty: false });
      await load(worktreePath, path);
    },

    close(worktreePath, path) {
      forgetDocument(documentKey(worktreePath, path));
      set((state) => {
        const files = state.files[worktreePath] ?? [];
        const index = files.findIndex((file) => file.path === path);
        const rest = files.filter((file) => file.path !== path);
        const wasActive = state.active[worktreePath] === path;
        const next = rest[Math.min(index, rest.length - 1)]?.path ?? null;
        return {
          files: { ...state.files, [worktreePath]: rest },
          active: wasActive
            ? { ...state.active, [worktreePath]: next }
            : state.active,
        };
      });
    },
  };
});

/** The file on screen in a worktree, if one is. */
export function activeFileOf(
  state: Pick<EditorStoreState, "files" | "active">,
  worktreePath: string | null,
): OpenFile | null {
  if (!worktreePath) return null;
  const path = state.active[worktreePath];
  if (!path) return null;
  return state.files[worktreePath]?.find((file) => file.path === path) ?? null;
}

// Choosing another terminal in a worktree brings its terminals back in front of the editor.
useTerminalStore.subscribe((state, previous) => {
  if (state.activeByWorktree === previous.activeByWorktree) return;
  for (const [worktreePath, id] of Object.entries(state.activeByWorktree)) {
    if (previous.activeByWorktree[worktreePath] !== id) {
      useEditorStore.getState().show(worktreePath, null);
    }
  }
});
