import { useEffect, useState } from "react";

/** Every row of the tree is this tall (`h-6`), which is what lets the list draw only a window of them. */
export const ROW_HEIGHT = 24;
/** Rows kept above and below what is visible, so a fast scroll does not show blanks. */
const OVERSCAN = 10;
/** Used until the scroll box is measured. */
const FALLBACK_HEIGHT = 800;

export interface ScrollWindow {
  /** Index of the first row at least partly visible. */
  first: number;
  /** How many rows fit in the box. */
  count: number;
}

/** The index range to draw: what is visible plus the overscan, cut to the list. */
export function windowRange(window: ScrollWindow, total: number) {
  return {
    start: Math.max(0, window.first - OVERSCAN),
    end: Math.min(total, window.first + window.count + OVERSCAN),
  };
}

/**
 * The indexes to draw, in order: the window, plus the row that holds the Tab stop. That row must
 * stay mounted even when scrolled away, or the keyboard would skip the whole tree.
 */
export function indexesToDraw(
  range: { start: number; end: number },
  pinned: number,
) {
  const indexes: number[] = [];
  const pinnedOutside =
    pinned >= 0 && (pinned < range.start || pinned >= range.end);
  if (pinnedOutside && pinned < range.start) indexes.push(pinned);
  for (let index = range.start; index < range.end; index++) indexes.push(index);
  if (pinnedOutside && pinned >= range.end) indexes.push(pinned);
  return indexes;
}

/** Follows a scroll box, and re-renders only when the first visible row or the row count changes. */
export function useScrollWindow(viewport: HTMLElement | null): ScrollWindow {
  const [window, setWindow] = useState<ScrollWindow>({
    first: 0,
    count: Math.ceil(FALLBACK_HEIGHT / ROW_HEIGHT),
  });

  useEffect(() => {
    if (!viewport) return;
    const measure = () => {
      const first = Math.floor(viewport.scrollTop / ROW_HEIGHT);
      const count = Math.ceil(viewport.clientHeight / ROW_HEIGHT) + 1;
      setWindow((previous) =>
        previous.first === first && previous.count === count
          ? previous
          : { first, count },
      );
    };
    measure();
    viewport.addEventListener("scroll", measure, { passive: true });
    const observer = new ResizeObserver(measure);
    observer.observe(viewport);
    return () => {
      viewport.removeEventListener("scroll", measure);
      observer.disconnect();
    };
  }, [viewport]);

  return window;
}

/** Scrolls the box just enough to show a row, as the browser would for a focused element. */
export function scrollRowIntoView(viewport: HTMLElement | null, index: number) {
  if (!viewport) return;
  const top = index * ROW_HEIGHT;
  const bottom = top + ROW_HEIGHT;
  if (top < viewport.scrollTop) viewport.scrollTop = top;
  else if (bottom > viewport.scrollTop + viewport.clientHeight)
    viewport.scrollTop = bottom - viewport.clientHeight;
}
