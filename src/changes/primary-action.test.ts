import { describe, expect, it } from "vitest";

import type { BranchSync } from "@/bindings";
import { primaryAction } from "@/changes/primary-action";

const sync = (change: Partial<BranchSync>): BranchSync => ({
  branch: "feat/x",
  upstream: "origin/feat/x",
  ahead: 0,
  behind: 0,
  remote: "origin",
  ...change,
});

describe("primaryAction", () => {
  it("commits while files are staged, even with commits waiting to be pushed", () => {
    expect(primaryAction(2, sync({ ahead: 3 }))).toEqual({ kind: "commit" });
  });

  it("pushes when the index is empty and the branch is ahead", () => {
    expect(primaryAction(0, sync({ ahead: 2 }))).toEqual({
      kind: "push",
      remote: "origin",
      ahead: 2,
      publish: false,
    });
  });

  it("publishes a branch the remote does not have yet", () => {
    expect(primaryAction(0, sync({ upstream: null }))).toMatchObject({
      kind: "push",
      publish: true,
    });
  });

  it("stays a commit when there is nothing to send or nowhere to send it", () => {
    expect(primaryAction(0, sync({}))).toEqual({ kind: "commit" });
    expect(primaryAction(0, sync({ remote: null, upstream: null }))).toEqual({
      kind: "commit",
    });
    expect(primaryAction(0, sync({ branch: null, ahead: 4 }))).toEqual({
      kind: "commit",
    });
    expect(primaryAction(0, undefined)).toEqual({ kind: "commit" });
  });
});
