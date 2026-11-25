import cytoscape from "cytoscape";
import {
  useCallback,
  useEffect,
  useMemo,
  useRef,
  useState,
  type PointerEvent,
} from "react";

import type { TeamMember } from "@/team/team-fixtures";
import {
  cardTransform,
  centreOf,
  cornerOf,
  focusOf,
  boxOf,
  centredPan,
  clampSize,
  fitView,
  handleAnchor,
  isInView,
  OPPOSITE_HANDLE,
  openSizeOf,
  sizeOf,
  type Box,
  type CardSize,
  type HandleName,
} from "@/team/team-canvas-model";
import {
  extraRowsAt,
  siblingOrderAt,
  TEAM_CARD_HEIGHT,
  TEAM_CARD_WIDTH,
  teamLayout,
  type SiblingOrder,
  type TeamPosition,
} from "@/team/team-layout";
import { isValidConnection, pairKey, type TeamPair } from "@/team/team-pairs";
import { canOrchestrate, linkOf } from "@/team/team-hierarchy";
import { cn } from "@/lib/utils";
import { Button } from "@/components/ui/button";
import { TeamNodeCard } from "@/team/TeamNodeCard";
import { openMember, useTeamStore } from "@/team/team-store";
import { useGlidedPositions } from "@/team/use-glided-positions";
import { copy } from "@/ui-copy";

/** Empty space around the team when the map fits the screen, in screen pixels. */
const FIT_PADDING = 40;
/** How far the pointer must move before a press on a card is a drag, in screen pixels. */
const DRAG_THRESHOLD = 4;
/** How long opening or closing a card's terminal takes, in ms, and the curve it follows. */
const CARD_OPEN_MS = 280;
const CARD_OPEN_EASING = "ease-in-out" as const;

/**
 * A computed colour of a design token. Cytoscape cannot read CSS variables, so the token is resolved by the browser
 * once, when the canvas is built.
 */
function tokenColor(token: string, fallback: string): string {
  const probe = document.createElement("span");
  probe.style.color = `var(${token})`;
  document.body.append(probe);
  const colour = getComputedStyle(probe).color;
  probe.remove();
  return colour || fallback;
}

function canvasStyle(): cytoscape.StylesheetStyle[] {
  const line = tokenColor("--muted-foreground", "#888888");
  const ink = tokenColor("--foreground", "#ffffff");
  return [
    // Cards are drawn over the canvas; a node here is only where a line starts and ends, so it is the card's size.
    {
      selector: "node",
      style: {
        width: TEAM_CARD_WIDTH,
        height: TEAM_CARD_HEIGHT,
        opacity: 0,
      },
    },
    {
      selector: "edge",
      style: {
        width: 1.5,
        "line-color": line,
        "curve-style": "taxi",
        "taxi-direction": "downward",
        "target-arrow-shape": "none",
        "source-arrow-shape": "none",
      },
    },
    {
      selector: "edge[kind = 'pair']",
      style: {
        "curve-style": "straight",
        "line-style": "dashed",
        "line-dash-pattern": [1, 6],
      },
    },
    { selector: "edge.focus", style: { "line-color": ink } },
    { selector: "edge.dim", style: { opacity: 0.25 } },
  ];
}

/** The lines the team has: each member's line to its orchestrator, and each pair, between cards that are placed. */
function linesOf(
  members: readonly TeamMember[],
  placed: ReadonlyMap<string, TeamPosition>,
  pairs: readonly TeamPair[],
): cytoscape.ElementDefinition[] {
  const lines: cytoscape.ElementDefinition[] = [];
  for (const member of members) {
    if (
      member.parentId &&
      placed.has(member.parentId) &&
      placed.has(member.id)
    ) {
      lines.push({
        group: "edges",
        data: {
          id: `${member.parentId}-${member.id}`,
          source: member.parentId,
          target: member.id,
          kind: "tree",
        },
      });
    }
  }
  for (const pair of pairs) {
    if (placed.has(pair.from) && placed.has(pair.to)) {
      lines.push({
        group: "edges",
        data: {
          id: pairKey(pair.from, pair.to),
          source: pair.from,
          target: pair.to,
          kind: "pair",
        },
      });
    }
  }
  return lines;
}

const HANDLE_PLACE: Record<HandleName, string> = {
  top: "-top-1 left-1/2 -translate-x-1/2",
  bottom: "-bottom-1 left-1/2 -translate-x-1/2",
  left: "top-1/2 -left-1 -translate-y-1/2",
  right: "top-1/2 -right-1 -translate-y-1/2",
};

/** A card's handle: it shows with the mouse over the card or focus inside it, and a press on it starts a line. */
function HandleDot({
  handle,
  onStart,
}: {
  handle: HandleName;
  onStart: (event: PointerEvent<HTMLDivElement>) => void;
}) {
  return (
    <div
      aria-hidden
      data-handle={handle}
      onPointerDown={onStart}
      className={cn(
        "pointer-events-auto absolute size-2.5 cursor-crosshair rounded-full border border-ring bg-background opacity-0 transition-opacity group-hover:opacity-100 group-focus-within:opacity-100",
        HANDLE_PLACE[handle],
      )}
    />
  );
}

const RESIZE_PLACE: Record<"right" | "bottom" | "corner", string> = {
  right: "top-0 -right-1 h-full w-2 cursor-ew-resize",
  bottom: "-bottom-1 left-0 h-2 w-full cursor-ns-resize",
  corner: "-right-1.5 -bottom-1.5 size-4 cursor-nwse-resize",
};

/** A card's edge that resizes it: an invisible strip on its border, so the card keeps its look. */
function ResizeDot({
  edge,
  onStart,
}: {
  edge: "right" | "bottom" | "corner";
  onStart: (event: PointerEvent<HTMLDivElement>) => void;
}) {
  return (
    <div
      aria-hidden
      data-resize={edge}
      onPointerDown={onStart}
      className={cn("pointer-events-auto absolute", RESIZE_PLACE[edge])}
    />
  );
}

/**
 * The team as a canvas. Cytoscape draws the lines and takes the pan, the zoom and the hit tests on empty space. The
 * cards are DOM laid over it, moved by the same pan and zoom, so a card keeps its own buttons and menus. A card taken
 * with the mouse leaves a dashed ghost where it would land, and the other cards make room for it.
 */
export function TeamCanvas({
  members,
  expanded,
  selectedId,
  extraRows,
  siblingOrder,
  pairs,
}: {
  members: readonly TeamMember[];
  expanded: readonly string[];
  selectedId: string | null;
  extraRows: Readonly<Record<string, number>>;
  siblingOrder: SiblingOrder;
  pairs: readonly TeamPair[];
}) {
  const surface = useRef<HTMLDivElement>(null);
  const cards = useRef(new Map<string, HTMLDivElement>());
  const ghost = useRef<HTMLDivElement>(null);
  // The line drawn from a handle to the pointer while a connection is made; read by the canvas's render.
  const linkDraw = useRef<{ anchor: TeamPosition; point: TeamPosition } | null>(
    null,
  );
  const linkLine = useRef<SVGLineElement>(null);
  // Where the ghost goes, read by the canvas's render: kept out of the render itself.
  const spotRef = useRef<TeamPosition | null>(null);
  const canvas = useRef<cytoscape.Core | null>(null);
  // The card being held and where its top-left corner is, in canvas units.
  const [drag, setDrag] = useState<{
    id: string;
    position: TeamPosition;
  } | null>(null);
  // A drop is followed by a click on the same card, which must not select it.
  const dropped = useRef(false);
  // The card being resized and the size it has now; the store keeps the size it ends at.
  const [resizing, setResizing] = useState<{
    id: string;
    size: CardSize;
  } | null>(null);
  const cardSizes = useTeamStore((state) => state.cardSizes);
  const canvasMode = useTeamStore((state) => state.canvasMode);
  const terminalOpen = useTeamStore((state) => state.terminalOpen);
  const terminalFor = useTeamStore((state) => state.terminalFor);
  const movingId = useTeamStore((state) => state.movingId);
  // The card whose terminal is open, in terminal mode. It grows to the size the modal had: 90% of the area.
  const expandedId =
    canvasMode === "terminal" && terminalOpen ? terminalFor : null;
  // The map's own area, measured as it changes: an open card is 90% of it in each direction.
  const [area, setArea] = useState<CardSize | null>(null);
  const expansion = useMemo<CardSize | null>(
    () =>
      area
        ? {
            width: Math.round(area.width * 0.9),
            height: Math.round(area.height * 0.9),
          }
        : null,
    [area],
  );
  const sizeNowOf = useCallback(
    (id: string): CardSize => {
      if (id === expandedId && expansion) return expansion;
      if (resizing?.id === id) return resizing.size;
      return sizeOf(cardSizes, id);
    },
    [expandedId, expansion, resizing, cardSizes],
  );
  // Read by the canvas's render, which is set up once and outlives every change: the card that is open, and whether
  // its terminal is on the screen (a terminal off the screen is not mounted).
  const openRef = useRef<string | null>(null);
  const terminalVisible = useRef(true);
  const [terminalShown, setTerminalShown] = useState(true);

  const base = useMemo(
    () => teamLayout(members, expanded, extraRows, siblingOrder, pairs),
    [members, expanded, extraRows, siblingOrder, pairs],
  );
  // The row the held card is over. While it is not its own, the others already take the places they would have
  // on release, and go back if it returns.
  const dragId = drag?.id ?? null;
  const rows = drag ? extraRowsAt(members, base, drag.id, drag.position.y) : 0;
  const moved = dragId !== null && rows !== (extraRows[dragId] ?? 0);
  const { targets, order } = useMemo(() => {
    if (!dragId || !drag) return { targets: base, order: siblingOrder };
    const rowsNow = moved ? { ...extraRows, [dragId]: rows } : extraRows;
    const rowLayout = moved
      ? teamLayout(members, expanded, rowsNow, siblingOrder, pairs)
      : base;
    // Sideways, the card goes after the siblings on its row that are to its left.
    const next = siblingOrderAt(
      members,
      rowLayout,
      dragId,
      drag.position.x,
      siblingOrder,
      pairs,
    );
    return next === siblingOrder
      ? { targets: rowLayout, order: next }
      : {
          targets: teamLayout(members, expanded, rowsNow, next, pairs),
          order: next,
        };
  }, [
    drag,
    dragId,
    moved,
    rows,
    members,
    expanded,
    extraRows,
    siblingOrder,
    pairs,
    base,
  ]);
  // Where each card is drawn: the others glide to their targets, the held card stays under the pointer.
  const shown = useGlidedPositions(targets, drag);
  const focus = useMemo(
    () => focusOf(members, pairs, selectedId),
    [members, pairs, selectedId],
  );

  // Read by a pointer release, which outlives the render it started in.
  const latest = useRef({ drag, base, members, siblingOrder, order, pairs });
  useEffect(() => {
    latest.current = { drag, base, members, siblingOrder, order, pairs };
  });

  // Built once. Its handlers read the store, so they never go stale.
  useEffect(() => {
    const element = surface.current;
    if (!element) return;
    const cy = cytoscape({
      container: element,
      style: canvasStyle(),
      minZoom: 0.2,
      maxZoom: 1.5,
      boxSelectionEnabled: false,
      autoungrabify: true,
    });
    canvas.current = cy;
    const observer = new ResizeObserver(() => {
      const box = element.getBoundingClientRect();
      setArea({ width: box.width, height: box.height });
    });
    observer.observe(element);

    // Each card follows the canvas: its corner in the canvas's units, scaled and moved by the same zoom and pan.
    const place = () => {
      const zoom = cy.zoom();
      const pan = cy.pan();
      // The canvas sizes each card, so a resize or an animation moves its contents along.
      for (const [id, card] of cards.current) {
        const node = cy.getElementById(id);
        if (node.empty()) continue;
        const { x, y } = node.position();
        const size = { width: node.width(), height: node.height() };
        card.style.width = `${size.width}px`;
        card.style.height = `${size.height}px`;
        card.style.transform = cardTransform(
          cornerOf({ x, y }, size),
          zoom,
          pan,
        );
      }
      const openNode = openRef.current
        ? cy.getElementById(openRef.current)
        : null;
      const onScreen =
        !openNode ||
        openNode.empty() ||
        isInView(
          boxOf(openNode.position(), {
            width: openNode.width(),
            height: openNode.height(),
          }),
          zoom,
          pan,
          { width: element.clientWidth, height: element.clientHeight },
        );
      if (onScreen !== terminalVisible.current) {
        terminalVisible.current = onScreen;
        setTerminalShown(onScreen);
      }
      const spotNow = spotRef.current;
      if (ghost.current && spotNow) {
        ghost.current.style.transform = cardTransform(spotNow, zoom, pan);
      }
      const line = linkLine.current;
      const draw = linkDraw.current;
      if (line) {
        if (draw) {
          const screenOf = (at: TeamPosition) => ({
            x: at.x * zoom + pan.x,
            y: at.y * zoom + pan.y,
          });
          const a = screenOf(draw.anchor);
          const b = screenOf(draw.point);
          line.setAttribute("x1", String(a.x));
          line.setAttribute("y1", String(a.y));
          line.setAttribute("x2", String(b.x));
          line.setAttribute("y2", String(b.y));
          line.style.display = "";
        } else {
          line.style.display = "none";
        }
      }
    };
    cy.on("render", place);
    cy.on("tap", (event) => {
      if (event.target === cy) useTeamStore.getState().select(null);
    });

    useTeamStore.setState({
      fitMap: () => cy.fit(cy.nodes(), FIT_PADDING),
    });
    return () => {
      useTeamStore.setState({ fitMap: null });
      canvas.current = null;
      observer.disconnect();
      cy.destroy();
    };
  }, []);

  // Opening a card's terminal grows the card to the size of the modal and centres the map on it; closing it lands the
  // card in its place and fits the map. Cytoscape moves the cards, and the DOM cards follow its render. The canvas's
  // sync below leaves a moving card alone until it has landed.
  useEffect(() => {
    const previous = openRef.current;
    openRef.current = expandedId;
    const cy = canvas.current;
    const element = surface.current;
    if (!cy || !element || previous === expandedId) return;
    const measured = element.getBoundingClientRect();
    const area = { width: measured.width, height: measured.height };
    const duration = window.matchMedia("(prefers-reduced-motion: reduce)")
      .matches
      ? 0
      : CARD_OPEN_MS;
    const motion = { duration, easing: CARD_OPEN_EASING, queue: false };
    const viewTo = (zoom: number, pan: { x: number; y: number }) => {
      cy.stop();
      if (duration === 0) cy.viewport({ zoom, pan });
      else cy.animate({ zoom, pan }, motion);
    };
    // The card takes its size and place. While it moves, its terminal keeps the process's size (see TerminalView).
    const moveCard = (id: string, size: CardSize, centre: TeamPosition) => {
      const node = cy.getElementById(id);
      if (node.empty()) return;
      node.stop();
      if (duration === 0) {
        node.position(centre);
        node.style({ width: size.width, height: size.height });
        return;
      }
      useTeamStore.setState({ movingId: id });
      node.animate(
        { position: centre, style: { width: size.width, height: size.height } },
        {
          ...motion,
          complete: () => {
            if (useTeamStore.getState().movingId === id) {
              useTeamStore.setState({ movingId: null });
            }
          },
        },
      );
    };

    // The card that was open goes back to its place first.
    let landed: Box | null = null;
    if (previous) {
      const corner = latest.current.base.get(previous);
      if (corner) {
        const size = sizeOf(useTeamStore.getState().cardSizes, previous);
        const centre = centreOf(corner, size);
        moveCard(previous, size, centre);
        landed = boxOf(centre, size);
      }
    }
    if (expandedId) {
      const node = cy.getElementById(expandedId);
      if (!node.empty()) {
        const centre = node.position();
        moveCard(expandedId, openSizeOf(area), centre);
        viewTo(1, centredPan(centre, area));
      }
      return;
    }
    // Closing: the map fits the team, with the card at the place it lands.
    const boxes: Box[] = landed ? [landed] : [];
    cy.nodes().forEach((node) => {
      if (node.id() === previous) return;
      boxes.push(
        boxOf(node.position(), { width: node.width(), height: node.height() }),
      );
    });
    const view = fitView(boxes, area, FIT_PADDING, {
      min: cy.minZoom(),
      max: cy.maxZoom(),
    });
    viewTo(view.zoom, view.pan);
  }, [expandedId]);

  // The open card stays centred when the map's area changes; while it grows, the animation above centres it.
  useEffect(() => {
    const cy = canvas.current;
    const element = surface.current;
    if (!cy || !element || !expandedId || !expansion) return;
    if (useTeamStore.getState().movingId) return;
    const node = cy.getElementById(expandedId);
    if (node.empty()) return;
    cy.viewport({
      zoom: 1,
      pan: centredPan(node.position(), {
        width: element.clientWidth,
        height: element.clientHeight,
      }),
    });
  }, [expandedId, expansion]);

  // The canvas takes where the cards are drawn: one node per card, and the lines between them.
  useEffect(() => {
    const cy = canvas.current;
    if (!cy) return;
    const stored = useTeamStore.getState();
    const moving = stored.movingId;
    // The open card keeps the centre the map was centred on; its size still follows the area.
    const openId =
      stored.canvasMode === "terminal" && stored.terminalOpen
        ? stored.terminalFor
        : null;
    const placed = new Map<string, TeamPosition>();
    for (const member of members) {
      const corner = shown.get(member.id) ?? targets.get(member.id);
      if (corner && base.has(member.id)) placed.set(member.id, corner);
    }
    cy.batch(() => {
      for (const [id, corner] of placed) {
        const size = sizeNowOf(id);
        const centre = centreOf(corner, size);
        const node = cy.getElementById(id);
        // The card that is moving is left to its animation, which lands it at its place.
        if (node.nonempty() && id === moving) continue;
        if (node.empty()) {
          cy.add({ group: "nodes", data: { id }, position: centre });
        } else if (id !== openId) {
          node.position(centre);
        }
        cy.getElementById(id).style({ width: size.width, height: size.height });
      }
      cy.nodes().forEach((node) => {
        if (!placed.has(node.id())) node.remove();
      });
      cy.edges().remove();
      cy.add(linesOf(members, placed, pairs));
    });
    // The lines the canvas draws, for the tests: the canvas draws them itself, so they are not in the DOM.
    cy.container()?.setAttribute("data-team-lines", String(cy.edges().length));
    cy.emit("render");
  }, [members, shown, targets, base, pairs, sizeNowOf, movingId]);

  // The lines of the selected member's line of work stay solid; the rest fade.
  useEffect(() => {
    const cy = canvas.current;
    if (!cy) return;
    cy.edges().forEach((edge) => {
      const inFocus =
        focus !== null &&
        focus.has(edge.source().id()) &&
        focus.has(edge.target().id());
      edge.toggleClass("focus", inFocus);
      edge.toggleClass("dim", focus !== null && !inFocus);
    });
  }, [focus, shown, pairs]);

  // A card added or removed can land outside the view, so the map fits again. The wait lets the canvas place the
  // new cards first; shortcut: a fixed 50 ms, upgrade to an event if a slow machine misses it.
  const cardIds = members
    .filter((member) => base.has(member.id))
    .map((member) => member.id)
    .join("|");
  useEffect(() => {
    const timer = window.setTimeout(
      () => useTeamStore.getState().fitMap?.(),
      50,
    );
    return () => window.clearTimeout(timer);
  }, [cardIds]);

  // The pointer's place in canvas units: the screen point, minus the canvas's pan, divided by its zoom.
  const onCanvas = (clientX: number, clientY: number): TeamPosition => {
    const box = surface.current?.getBoundingClientRect();
    const cy = canvas.current;
    const zoom = cy?.zoom() ?? 1;
    const pan = cy?.pan() ?? { x: 0, y: 0 };
    return {
      x: (clientX - (box?.left ?? 0) - pan.x) / zoom,
      y: (clientY - (box?.top ?? 0) - pan.y) / zoom,
    };
  };

  // A press on a card is a drag once the pointer has moved past the threshold; a press on a button is not.
  const startDrag = (id: string, event: PointerEvent<HTMLDivElement>) => {
    // Each press starts afresh: a drop that no click followed must not swallow the next click.
    dropped.current = false;
    if (event.button !== 0) return;
    if ((event.target as Element).closest("button")) return;
    // The open card holds its terminal: a drag from it would take the terminal's presses.
    if (id === expandedId) return;
    const corner = base.get(id);
    if (!corner) return;
    const origin = { x: event.clientX, y: event.clientY };
    const start = onCanvas(event.clientX, event.clientY);
    const offset = { x: start.x - corner.x, y: start.y - corner.y };
    let moving = false;
    const move = (moment: globalThis.PointerEvent) => {
      if (
        !moving &&
        Math.hypot(moment.clientX - origin.x, moment.clientY - origin.y) <
          DRAG_THRESHOLD
      ) {
        return;
      }
      moving = true;
      // A drag is not text: a selection the press began is dropped.
      window.getSelection()?.removeAllRanges();
      const at = onCanvas(moment.clientX, moment.clientY);
      setDrag({
        id,
        position: { x: at.x - offset.x, y: at.y - offset.y },
      });
    };
    const release = () => {
      window.removeEventListener("pointermove", move);
      window.removeEventListener("pointerup", release);
      window.removeEventListener("pointercancel", release);
      if (!moving) return;
      dropped.current = true;
      // The row and the place the card is let go in are what the team keeps; the others glide to them.
      const held = latest.current;
      if (!held.drag) return;
      const store = useTeamStore.getState();
      store.setExtraRows(
        held.drag.id,
        extraRowsAt(
          held.members,
          held.base,
          held.drag.id,
          held.drag.position.y,
        ),
      );
      if (held.order !== held.siblingOrder) {
        store.setSiblingOrder(held.order);
      }
      setDrag(null);
    };
    // On the window, not the card: a fast pointer that leaves the card still moves it, and no capture retargets the click.
    window.addEventListener("pointermove", move);
    window.addEventListener("pointerup", release);
    window.addEventListener("pointercancel", release);
  };

  // A press on a card's right side, bottom or corner resizes it: the size follows the pointer in canvas units, and
  // the store keeps the size it ends at. The card's centre moves with its corner, so the lines follow it.
  const startResize = (
    id: string,
    edge: "right" | "bottom" | "corner",
    event: PointerEvent<HTMLDivElement>,
  ) => {
    event.stopPropagation();
    if (event.button !== 0) return;
    const first = sizeNowOf(id);
    const origin = onCanvas(event.clientX, event.clientY);
    let last = first;
    const move = (moment: globalThis.PointerEvent) => {
      const at = onCanvas(moment.clientX, moment.clientY);
      last = clampSize({
        width:
          edge === "bottom" ? first.width : first.width + (at.x - origin.x),
        height:
          edge === "right" ? first.height : first.height + (at.y - origin.y),
      });
      setResizing({ id, size: last });
    };
    const release = () => {
      window.removeEventListener("pointermove", move);
      window.removeEventListener("pointerup", release);
      window.removeEventListener("pointercancel", release);
      useTeamStore.getState().setCardSize(id, last);
      setResizing(null);
    };
    window.addEventListener("pointermove", move);
    window.addEventListener("pointerup", release);
    window.addEventListener("pointercancel", release);
  };

  // A connection the team accepts: a line between two handles means what linkOf says it means, and the team keeps it
  // only when the rule for that kind of line allows it.
  const connectCards = (connection: {
    source: string;
    target: string;
    sourceHandle: string;
    targetHandle: string;
  }) => {
    const { members: now, pairs: linked } = latest.current;
    const link = linkOf(connection);
    if (!link) return;
    const allowed =
      link.kind === "orchestrates"
        ? canOrchestrate(now, link.parent, link.child)
        : isValidConnection(
            { source: link.from, target: link.to },
            now,
            linked,
          );
    if (!allowed) return;
    const store = useTeamStore.getState();
    if (link.kind === "orchestrates") store.setParent(link.child, link.parent);
    else store.setLinked(link.from, link.to, true);
  };

  // A press on a handle draws a line from it to the pointer; the card it is let go on takes the line.
  const startLink = (
    from: string,
    handle: HandleName,
    event: PointerEvent<HTMLDivElement>,
  ) => {
    // The press is for the handle, not the card: no drag starts.
    event.stopPropagation();
    const cy = canvas.current;
    if (event.button !== 0 || !cy) return;
    const node = cy.getElementById(from);
    if (node.empty()) return;
    const size = { width: node.width(), height: node.height() };
    const anchor = handleAnchor(cornerOf(node.position(), size), handle, size);
    const drawTo = (clientX: number, clientY: number) => {
      linkDraw.current = { anchor, point: onCanvas(clientX, clientY) };
      cy.emit("render");
    };
    const cancel = () => {
      window.removeEventListener("pointermove", move);
      window.removeEventListener("pointerup", release);
      window.removeEventListener("pointercancel", cancel);
      linkDraw.current = null;
      cy.emit("render");
    };
    const move = (moment: globalThis.PointerEvent) =>
      drawTo(moment.clientX, moment.clientY);
    const release = (moment: globalThis.PointerEvent) => {
      cancel();
      // A release on the target's handle counts as well as on its body: the handles sit beside the card, in its wrapper.
      const to = document
        .elementFromPoint(moment.clientX, moment.clientY)
        ?.closest<HTMLElement>("[data-owner]")?.dataset.owner;
      if (to && to !== from) {
        connectCards({
          source: from,
          target: to,
          sourceHandle: handle,
          targetHandle: OPPOSITE_HANDLE[handle],
        });
      }
    };
    drawTo(event.clientX, event.clientY);
    window.addEventListener("pointermove", move);
    window.addEventListener("pointerup", release);
    window.addEventListener("pointercancel", cancel);
  };

  // Delete (or Backspace) removes the picked lines: a tree line takes its member off its orchestrator, a pair is
  // unpaired. Typing in a field is left alone.
  useEffect(() => {
    const onKey = (event: KeyboardEvent) => {
      if (event.key !== "Delete" && event.key !== "Backspace") return;
      if (
        (event.target as HTMLElement).closest(
          "input, textarea, [contenteditable='true']",
        )
      ) {
        return;
      }
      const cy = canvas.current;
      const picked = cy?.edges(":selected");
      if (!cy || !picked || picked.empty()) return;
      const store = useTeamStore.getState();
      picked.forEach((edge) => {
        if (edge.data("kind") === "tree") {
          store.setParent(edge.target().id(), null);
        } else {
          store.setLinked(edge.source().id(), edge.target().id(), false);
        }
      });
      event.preventDefault();
    };
    window.addEventListener("keydown", onKey);
    return () => window.removeEventListener("keydown", onKey);
  }, []);

  // Where the held card would land: the spot under its parent, once the others have made room. The ghost is placed
  // by the canvas's own render, so it follows the zoom and the pan without a new render.
  const spot = targets !== base && dragId ? targets.get(dragId) : undefined;
  useEffect(() => {
    spotRef.current = spot ?? null;
    canvas.current?.emit("render");
  }, [spot]);

  return (
    <div className="relative size-full" aria-label={copy.team.mapLabel}>
      <div className="absolute top-2 right-2 z-20">
        <Button
          size="sm"
          variant="outline"
          aria-pressed={canvasMode === "terminal"}
          onClick={() =>
            useTeamStore
              .getState()
              .setCanvasMode(canvasMode === "nodes" ? "terminal" : "nodes")
          }
        >
          {copy.team.canvasMode.label}
        </Button>
      </div>
      {/* Cytoscape sets the position of its own container, so the box that sizes it is this one. */}
      <div className="absolute inset-0">
        <div ref={surface} className="size-full" />
      </div>
      <div className="pointer-events-none absolute inset-0 overflow-hidden">
        {spot && (
          <div
            ref={ghost}
            aria-hidden
            className="absolute top-0 left-0 origin-top-left rounded-lg border-2 border-dashed border-ring/70 bg-ring/10"
            style={{ width: TEAM_CARD_WIDTH, height: TEAM_CARD_HEIGHT }}
          />
        )}
        <svg
          aria-hidden
          className="pointer-events-none absolute inset-0 size-full overflow-visible"
        >
          <line
            ref={linkLine}
            className="stroke-ring"
            strokeWidth={1.5}
            strokeDasharray="4 4"
            style={{ display: "none" }}
          />
        </svg>
        {members.flatMap((member) => {
          if (!base.has(member.id)) return [];
          return [
            <div
              key={member.id}
              data-owner={member.id}
              ref={(element) => {
                if (element) cards.current.set(member.id, element);
                else cards.current.delete(member.id);
              }}
              onPointerDown={(event) => startDrag(member.id, event)}
              onClickCapture={(event) => {
                // The click that ends a drag is not a pick.
                if (!dropped.current) return;
                dropped.current = false;
                event.stopPropagation();
              }}
              className={cn(
                "group pointer-events-auto absolute top-0 left-0 origin-top-left",
                member.id === expandedId && "z-10",
              )}
            >
              <TeamNodeCard
                member={member}
                hasChildren={members.some(
                  (other) => other.parentId === member.id,
                )}
                expanded={expanded.includes(member.id)}
                selected={member.id === selectedId}
                dimmed={focus !== null && !focus.has(member.id)}
                open={expandedId === member.id}
                terminalVisible={terminalShown}
                terminal={canvasMode === "terminal"}
                onClose={() => useTeamStore.getState().closeTerminal()}
                onOpen={() => {
                  useTeamStore.getState().select(member.id);
                  openMember(member.id);
                }}
              />
              {expandedId !== member.id && (
                <>
                  <ResizeDot
                    edge="right"
                    onStart={(event) => startResize(member.id, "right", event)}
                  />
                  <ResizeDot
                    edge="bottom"
                    onStart={(event) => startResize(member.id, "bottom", event)}
                  />
                  <ResizeDot
                    edge="corner"
                    onStart={(event) => startResize(member.id, "corner", event)}
                  />
                  {member.kind === "agent" && (
                    <HandleDot
                      handle="top"
                      onStart={(event) => startLink(member.id, "top", event)}
                    />
                  )}
                  {member.orchestrator && (
                    <HandleDot
                      handle="bottom"
                      onStart={(event) => startLink(member.id, "bottom", event)}
                    />
                  )}
                  <HandleDot
                    handle="left"
                    onStart={(event) => startLink(member.id, "left", event)}
                  />
                  <HandleDot
                    handle="right"
                    onStart={(event) => startLink(member.id, "right", event)}
                  />
                </>
              )}
            </div>,
          ];
        })}
      </div>
    </div>
  );
}
