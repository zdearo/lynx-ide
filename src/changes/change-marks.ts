import type { ChangedFile, ChangeKind } from "@/bindings";

/** What the file tree shows: each changed file's kind, and the folders that hold a change. */
export interface ChangeMarks {
  files: Map<string, ChangeKind>;
  folders: Set<string>;
}

/**
 * The kind a file shows in the tree: the unstaged change when there is one, since that is what
 * is on disk now. A conflict wins over both.
 */
function kindOf(file: ChangedFile): ChangeKind | null {
  if (file.unstaged === "conflicted") return "conflicted";
  return file.unstaged ?? file.staged;
}

/** Paths are relative to the worktree and use "/", as Git prints them. */
export function changeMarks(files: readonly ChangedFile[]): ChangeMarks {
  const marks: ChangeMarks = { files: new Map(), folders: new Set() };
  for (const file of files) {
    const kind = kindOf(file);
    if (kind) marks.files.set(file.path, kind);
    const parts = file.path.split("/");
    for (let depth = 1; depth < parts.length; depth += 1) {
      marks.folders.add(parts.slice(0, depth).join("/"));
    }
  }
  return marks;
}
