import { describe, expect, it } from "vitest";

import type { Folder } from "@/bindings";
import {
  directoryNameOf,
  isLinkedRoot,
  isMirrored,
} from "@/workspace/linked-folders";

const folder = (id: string, linkedRootId: string | null): Folder => ({
  id,
  name: id,
  parentId: null,
  collapsed: false,
  sourcePath: linkedRootId ? `/code/${id}` : null,
  linkedRootId,
  missing: false,
});

describe("linked folder roles", () => {
  it("tells the linked folder from the folders it made and from ordinary ones", () => {
    const root = folder("root", "root");
    const child = folder("child", "root");
    const plain = folder("plain", null);

    expect([root, child, plain].map(isLinkedRoot)).toEqual([
      true,
      false,
      false,
    ]);
    expect([root, child, plain].map(isMirrored)).toEqual([false, true, false]);
  });
});

describe("directoryNameOf", () => {
  it("takes the last part of a path with either separator", () => {
    expect(directoryNameOf("/home/dev/Code")).toBe("Code");
    expect(directoryNameOf("/home/dev/Code/")).toBe("Code");
    expect(directoryNameOf("C:\\Users\\dev\\Code")).toBe("Code");
    expect(directoryNameOf("/")).toBe("/");
  });
});
