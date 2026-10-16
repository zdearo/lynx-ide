import type { FileEntry } from "@/bindings";

export type DirectoryStatus = "loading" | "ready" | "error";

export interface DirectoryState {
  status: DirectoryStatus;
  entries: FileEntry[];
  truncated: boolean;
  error: string | null;
}

/** What the explorer remembers for one worktree. Paths are relative, `/`-separated; "" is the root. */
export interface TreeState {
  directories: Record<string, DirectoryState>;
  expanded: Record<string, true>;
  selected: string | null;
}

export interface EntryRow {
  type: "entry";
  path: string;
  name: string;
  depth: number;
  isDirectory: boolean;
  symlink: boolean;
  ignored: boolean;
  expanded: boolean;
  loading: boolean;
}

/** A line that explains a folder instead of showing a file. */
export interface NoteRow {
  type: "note";
  /** Same path space as entries, so keys stay unique. */
  path: string;
  depth: number;
  kind: "empty" | "error" | "truncated";
  text: string;
}

export type TreeRow = EntryRow | NoteRow;

export function childPath(directory: string, name: string): string {
  return directory ? `${directory}/${name}` : name;
}

/** The rows to draw, top to bottom, for the folders that are open. */
export function visibleRows(tree: TreeState): TreeRow[] {
  const rows: TreeRow[] = [];

  const walk = (directory: string, depth: number) => {
    const state = tree.directories[directory];
    if (!state) return;

    for (const entry of state.entries) {
      const path = childPath(directory, entry.name);
      const isDirectory = entry.kind === "directory";
      const expanded = isDirectory && tree.expanded[path] === true;
      rows.push({
        type: "entry",
        path,
        name: entry.name,
        depth,
        isDirectory,
        symlink: entry.symlink,
        ignored: entry.ignored,
        expanded,
        loading: expanded && tree.directories[path]?.status === "loading",
      });
      if (expanded) walk(path, depth + 1);
    }

    if (state.truncated) {
      rows.push({
        type: "note",
        path: `${directory}\0truncated`,
        depth,
        kind: "truncated",
        text: "More items than are shown",
      });
    }
    if (state.status === "error") {
      rows.push({
        type: "note",
        path: `${directory}\0error`,
        depth,
        kind: "error",
        text: state.error ?? "Could not read this folder",
      });
    }
    if (state.status === "ready" && state.entries.length === 0) {
      rows.push({
        type: "note",
        path: `${directory}\0empty`,
        depth,
        kind: "empty",
        text: "Empty",
      });
    }
  };

  walk("", 0);
  return rows;
}
