import type { ChangeKind, ChangedFile } from "@/bindings";

export type ChangeSectionId = "conflicts" | "staged" | "changes" | "untracked";

export interface ChangeEntry {
  file: ChangedFile;
  /** The change this row shows: the staged one in "staged", the unstaged one elsewhere. */
  kind: ChangeKind;
}

export interface ChangeSection {
  id: ChangeSectionId;
  entries: ChangeEntry[];
}

/** Orca's order: what blocks a commit first, then what is staged, then the rest. */
const ORDER: readonly ChangeSectionId[] = [
  "conflicts",
  "staged",
  "changes",
  "untracked",
];

/** A file with both a staged and an unstaged change is in both sections, as in Git. */
export function changeSections(files: readonly ChangedFile[]): ChangeSection[] {
  const sections = new Map<ChangeSectionId, ChangeEntry[]>(
    ORDER.map((id) => [id, []]),
  );
  for (const file of files) {
    if (file.unstaged === "conflicted") {
      sections.get("conflicts")?.push({ file, kind: "conflicted" });
      continue;
    }
    if (file.staged) {
      sections.get("staged")?.push({ file, kind: file.staged });
    }
    if (file.unstaged === "untracked") {
      sections.get("untracked")?.push({ file, kind: "untracked" });
    } else if (file.unstaged) {
      sections.get("changes")?.push({ file, kind: file.unstaged });
    }
  }
  return ORDER.map((id) => ({ id, entries: sections.get(id) ?? [] })).filter(
    (section) => section.entries.length > 0,
  );
}

/** The one-letter mark of a change, as in `git status --short`. */
export function changeLetter(kind: ChangeKind): string {
  switch (kind) {
    case "added":
      return "A";
    case "modified":
      return "M";
    case "deleted":
      return "D";
    case "renamed":
      return "R";
    case "copied":
      return "C";
    case "typeChanged":
      return "T";
    case "untracked":
      return "U";
    case "conflicted":
      return "!";
  }
}

/** The colour family of a change: something new, something that moved, or something gone. */
export type ChangeTone = "added" | "modified" | "removed";

export function changeTone(kind: ChangeKind): ChangeTone {
  switch (kind) {
    case "added":
    case "untracked":
    case "copied":
      return "added";
    case "deleted":
    case "conflicted":
      return "removed";
    case "modified":
    case "renamed":
    case "typeChanged":
      return "modified";
  }
}
