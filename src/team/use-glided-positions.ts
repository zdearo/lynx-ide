import { useEffect, useRef, useState } from "react";

import type { TeamPosition } from "@/team/team-layout";

const GLIDE_MS = 280;

/** Where each card is `t` (0 to 1) of the way from `from` to `to`, easing out. A card with no start goes straight to its target. */
export function glidePositions(
  from: ReadonlyMap<string, TeamPosition>,
  to: ReadonlyMap<string, TeamPosition>,
  t: number,
): Map<string, TeamPosition> {
  const eased = 1 - (1 - t) ** 3;
  const next = new Map<string, TeamPosition>();
  for (const [id, target] of to) {
    const start = from.get(id) ?? target;
    next.set(id, {
      x: start.x + (target.x - start.x) * eased,
      y: start.y + (target.y - start.y) * eased,
    });
  }
  return next;
}

/** Whether every card but `except` (the one held, which the pointer places) is already where `to` has it. */
function sameSpot(
  from: ReadonlyMap<string, TeamPosition>,
  to: ReadonlyMap<string, TeamPosition>,
  except: string | null,
): boolean {
  for (const [id, target] of to) {
    if (id === except) continue;
    const start = from.get(id);
    if (start && (start.x !== target.x || start.y !== target.y)) return false;
  }
  return true;
}

/**
 * The positions to draw: the cards glide to `targets` whenever those change, so a drop, a collapse or a
 * change of plan during a drag never jumps. The lines are drawn from these positions too, so they follow
 * the cards instead of running ahead. A card being held follows the pointer and is left out of the glide;
 * on release it glides from where the pointer left it.
 */
export function useGlidedPositions(
  targets: ReadonlyMap<string, TeamPosition>,
  held: { id: string; position: TeamPosition } | null,
): ReadonlyMap<string, TeamPosition> {
  const [shown, setShown] = useState(targets);
  // What the last frame drew, for a glide that is cut short to start from there.
  const latest = useRef(shown);
  // The card held now, read by the glide's frames so it keeps following the pointer.
  const heldNow = useRef(held);

  // The held card's spot is kept in `shown` itself, so on release it is already where the pointer left it.
  let current = shown;
  if (held) {
    const at = shown.get(held.id);
    if (at?.x !== held.position.x || at?.y !== held.position.y) {
      current = new Map(shown).set(held.id, held.position);
      setShown(current);
    }
  }

  useEffect(() => {
    latest.current = shown;
    heldNow.current = held;
  });

  useEffect(() => {
    if (sameSpot(latest.current, targets, heldNow.current?.id ?? null)) return;
    const from = latest.current;
    const duration = window.matchMedia("(prefers-reduced-motion: reduce)")
      .matches
      ? 0
      : GLIDE_MS;
    const startedAt = performance.now();
    let frame = 0;
    const step = (now: number) => {
      const t = duration === 0 ? 1 : Math.min(1, (now - startedAt) / duration);
      const next = glidePositions(from, targets, t);
      const holding = heldNow.current;
      if (holding) next.set(holding.id, holding.position);
      setShown(next);
      if (t < 1) frame = requestAnimationFrame(step);
    };
    frame = requestAnimationFrame(step);
    return () => cancelAnimationFrame(frame);
  }, [targets]);

  return current;
}
