import { useEffect, useRef } from "react";

/**
 * Calls `refresh(target)` now, again every `intervalMs` while the window is visible, and when the
 * window gets focus. A new `target` restarts it; `null` turns it off.
 */
export function useRefreshWhileVisible(
  target: string | null,
  refresh: (target: string) => void,
  intervalMs: number,
) {
  // The effect below reads the latest function without restarting the timer on every render.
  const latest = useRef(refresh);
  useEffect(() => {
    latest.current = refresh;
  });

  useEffect(() => {
    if (target === null) return;
    const run = () => latest.current(target);
    run();
    const timer = window.setInterval(() => {
      if (document.visibilityState === "visible") run();
    }, intervalMs);
    window.addEventListener("focus", run);
    return () => {
      window.clearInterval(timer);
      window.removeEventListener("focus", run);
    };
  }, [target, intervalMs]);
}
