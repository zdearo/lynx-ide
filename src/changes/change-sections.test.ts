import { describe, expect, it } from "vitest";

import type { ChangedFile } from "@/bindings";
import {
  changeLetter,
  changeSections,
  changeTone,
} from "@/changes/change-sections";

const file = (
  path: string,
  staged: ChangedFile["staged"],
  unstaged: ChangedFile["unstaged"],
): ChangedFile => ({
  path,
  originalPath: null,
  staged,
  unstaged,
  stagedLines: null,
  unstagedLines: null,
});

describe("changeSections", () => {
  it("splits staged, changed and untracked files, in that order", () => {
    const sections = changeSections([
      file("u.txt", null, "untracked"),
      file("m.txt", null, "modified"),
      file("s.txt", "added", null),
    ]);

    expect(sections.map((section) => section.id)).toEqual([
      "staged",
      "changes",
      "untracked",
    ]);
    expect(sections[0].entries[0]).toMatchObject({ kind: "added" });
  });

  it("lists a file with both kinds of change in both sections", () => {
    const sections = changeSections([file("a.txt", "modified", "modified")]);

    expect(sections.map((section) => section.id)).toEqual([
      "staged",
      "changes",
    ]);
  });

  it("puts conflicts first and only there", () => {
    const sections = changeSections([
      file("m.txt", null, "modified"),
      file("c.txt", null, "conflicted"),
    ]);

    expect(sections[0].id).toBe("conflicts");
    expect(sections[0].entries.map((entry) => entry.file.path)).toEqual([
      "c.txt",
    ]);
    expect(sections).toHaveLength(2);
  });

  it("has no sections for a clean worktree", () => {
    expect(changeSections([])).toEqual([]);
  });
});

describe("changeLetter", () => {
  it("uses the letters of git status", () => {
    expect(changeLetter("modified")).toBe("M");
    expect(changeLetter("untracked")).toBe("U");
    expect(changeLetter("deleted")).toBe("D");
  });
});

describe("changeTone", () => {
  it("groups the kinds into added, modified and removed", () => {
    expect(changeTone("added")).toBe("added");
    expect(changeTone("untracked")).toBe("added");
    expect(changeTone("modified")).toBe("modified");
    expect(changeTone("renamed")).toBe("modified");
    expect(changeTone("deleted")).toBe("removed");
    expect(changeTone("conflicted")).toBe("removed");
  });
});
