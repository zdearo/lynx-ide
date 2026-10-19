import { describe, expect, it } from "vitest";

import type { ChangedFile } from "@/bindings";
import { changeMarks } from "@/changes/change-marks";

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

describe("changeMarks", () => {
  it("marks a file with the kind on disk, and a conflict over everything", () => {
    const marks = changeMarks([
      file("a.ts", "added", "modified"),
      file("b.ts", "modified", null),
      file("c.ts", null, "conflicted"),
      file("d.ts", "added", "conflicted"),
    ]);

    expect(marks.files.get("a.ts")).toBe("modified");
    expect(marks.files.get("b.ts")).toBe("modified");
    expect(marks.files.get("c.ts")).toBe("conflicted");
    expect(marks.files.get("d.ts")).toBe("conflicted");
  });

  it("marks every folder above a changed file, and no file as a folder", () => {
    const marks = changeMarks([file("src/ui/Button.tsx", null, "modified")]);

    expect([...marks.folders].sort()).toEqual(["src", "src/ui"]);
    expect(marks.folders.has("src/ui/Button.tsx")).toBe(false);
  });

  it("is empty when nothing changed", () => {
    const marks = changeMarks([]);

    expect(marks.files.size).toBe(0);
    expect(marks.folders.size).toBe(0);
  });
});
