import { describe, expect, it } from "vitest";

import {
  MODAL_FRAME_CLASS,
  MODAL_INSET_PERCENT,
  MODAL_TITLE_PX,
  STAGE_CLASS,
  sessionPlacement,
  sessionWrapperClass,
  stageBox,
  stageIdOf,
} from "@/team/team-stage";

describe("the stage's box", () => {
  it("sits in the modal's body: the inset on three sides and the title bar on top", () => {
    expect(stageBox({ width: 1000, height: 800 })).toEqual({
      top: 40 + MODAL_TITLE_PX,
      right: 50,
      bottom: 40,
      left: 50,
    });
  });

  it("keeps its classes in step with the numbers it is built from", () => {
    expect(STAGE_CLASS).toContain(
      `top-[calc(${MODAL_INSET_PERCENT}%+${MODAL_TITLE_PX}px)]`,
    );
    for (const side of ["right", "bottom", "left"]) {
      expect(STAGE_CLASS).toContain(`${side}-[${MODAL_INSET_PERCENT}%]`);
    }
    expect(MODAL_FRAME_CLASS).toContain(`inset-[${MODAL_INSET_PERCENT}%]`);
  });
});

describe("where a session is drawn", () => {
  it("draws every session in the tabs outside the team", () => {
    expect(sessionPlacement(false, false)).toBe("tabs");
    expect(sessionPlacement(false, true)).toBe("tabs");
  });

  it("draws an agent of the team on the stage, and every other session hidden", () => {
    expect(sessionPlacement(true, true)).toBe("stage");
    expect(sessionPlacement(true, false)).toBe("hidden");
  });

  it("gives the tabs their full area when shown, and hides them otherwise", () => {
    expect(sessionWrapperClass("tabs", true)).toBe("absolute inset-0");
    expect(sessionWrapperClass("tabs", false)).toBe("absolute inset-0 hidden");
    expect(sessionWrapperClass("hidden", false)).toBe(
      "absolute inset-0 hidden",
    );
  });

  it("keeps the stage's box, and takes no clicks while its terminal is not shown", () => {
    expect(sessionWrapperClass("stage", true)).toBe(STAGE_CLASS);
    expect(sessionWrapperClass("stage", false)).toBe(
      `${STAGE_CLASS} pointer-events-none`,
    );
  });

  it("names a stage element by its session", () => {
    expect(stageIdOf("abc")).toBe("team-stage-abc");
  });
});
