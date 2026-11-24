/**
 * The team's stage: the terminals of the agents in the selected worktree's team, drawn in the body of the team's modal.
 * Each one stays mounted, so opening and closing the modal changes only which one is visible.
 */

/** How far the modal's frame sits from each side of the central area, in percent. */
export const MODAL_INSET_PERCENT = 5;
/** The height of the modal's title bar (`h-9`). The stage starts right below it. */
export const MODAL_TITLE_PX = 36;

/** The frame: the title bar takes clicks; the body is transparent so the stage under it shows through. */
export const MODAL_FRAME_CLASS =
  "absolute inset-[5%] z-50 flex flex-col pointer-events-none";
/** The backdrop between the map and the stage: a click on it closes the modal. */
export const MODAL_OVERLAY_CLASS = "absolute inset-0 z-40 bg-black/10";
/** The stage: its box is the modal's body, so the terminals sit exactly where the body is. */
export const STAGE_CLASS =
  "absolute top-[calc(5%+36px)] right-[5%] bottom-[5%] left-[5%] z-[45]";

/** Distances from the edges of the central area. */
export interface Box {
  top: number;
  right: number;
  bottom: number;
  left: number;
}

/** The stage's box inside a central area of this size. */
export function stageBox(area: { width: number; height: number }): Box {
  const inset = MODAL_INSET_PERCENT / 100;
  return {
    top: area.height * inset + MODAL_TITLE_PX,
    right: area.width * inset,
    bottom: area.height * inset,
    left: area.width * inset,
  };
}

/** Where a session's view is drawn: the terminal tabs, the team's stage, or nowhere while the team is on screen. */
export type SessionPlacement = "tabs" | "stage" | "hidden";

/** The placement of a session: the team shows the stage for its agents, and hides every other session. */
export function sessionPlacement(
  teamMode: boolean,
  staged: boolean,
): SessionPlacement {
  if (!teamMode) return "tabs";
  return staged ? "stage" : "hidden";
}

/**
 * The class of one session's wrapper in the central area. The stage keeps its box always; while its terminal is not
 * shown, the box takes no clicks, so the map under it still does.
 */
export function sessionWrapperClass(
  placement: SessionPlacement,
  shown: boolean,
): string {
  if (placement === "stage") {
    return shown ? STAGE_CLASS : `${STAGE_CLASS} pointer-events-none`;
  }
  return shown ? "absolute inset-0" : "absolute inset-0 hidden";
}

/** The id of a session's stage element, which the modal names in its `aria-owns`. */
export function stageIdOf(sessionId: string): string {
  return `team-stage-${sessionId}`;
}
