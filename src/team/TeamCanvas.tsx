import cytoscape from "cytoscape";
import { useEffect, useMemo, useRef } from "react";

import type { TeamMember } from "@/team/team-fixtures";
import {
  TEAM_CARD_HEIGHT,
  TEAM_CARD_WIDTH,
  teamLayout,
  type SiblingOrder,
  type TeamPosition,
} from "@/team/team-layout";
import { pairKey, type TeamPair } from "@/team/team-pairs";
import {
  cardTransform,
  centreOf,
  cornerOf,
  focusOf,
} from "@/team/team-canvas-model";
import { TeamNodeCard } from "@/team/TeamNodeCard";
import { openMember, useTeamStore } from "@/team/team-store";
import { copy } from "@/ui-copy";

/** Empty space around the team when the map fits the screen, in screen pixels. */
const FIT_PADDING = 40;

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

/**
 * The team as a canvas. Cytoscape draws the lines and takes the pan, the zoom and the hit tests on empty space. The
 * cards are DOM laid over it, moved by the same pan and zoom, so a card keeps its own buttons and menus.
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
  const layer = useRef<HTMLDivElement>(null);
  const cards = useRef(new Map<string, HTMLDivElement>());
  const canvas = useRef<cytoscape.Core | null>(null);

  const placed = useMemo(
    () => teamLayout(members, expanded, extraRows, siblingOrder, pairs),
    [members, expanded, extraRows, siblingOrder, pairs],
  );
  const focus = useMemo(
    () => focusOf(members, pairs, selectedId),
    [members, pairs, selectedId],
  );

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

    // Each card follows the canvas: its corner in the canvas's units, scaled and moved by the same zoom and pan.
    const place = () => {
      const zoom = cy.zoom();
      const pan = cy.pan();
      for (const [id, card] of cards.current) {
        const node = cy.getElementById(id);
        if (node.empty()) continue;
        const { x, y } = node.position();
        card.style.transform = cardTransform(cornerOf({ x, y }), zoom, pan);
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
      cy.destroy();
    };
  }, []);

  // The canvas takes the placement of the cards: one node per placed card, and the lines between them.
  useEffect(() => {
    const cy = canvas.current;
    if (!cy) return;
    cy.batch(() => {
      for (const [id, corner] of placed) {
        const centre = centreOf(corner);
        const node = cy.getElementById(id);
        if (node.empty())
          cy.add({ group: "nodes", data: { id }, position: centre });
        else node.position(centre);
      }
      cy.nodes().forEach((node) => {
        if (!placed.has(node.id())) node.remove();
      });
      cy.edges().remove();
      cy.add(linesOf(members, placed, pairs));
    });
    cy.emit("render");
  }, [members, placed, pairs]);

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
  }, [focus, placed, pairs]);

  // A card added or removed can land outside the view, so the map fits again. The wait lets the canvas place the
  // new cards first; shortcut: a fixed 50 ms, upgrade to an event if a slow machine misses it.
  const cardIds = members
    .filter((member) => placed.has(member.id))
    .map((member) => member.id)
    .join("|");
  useEffect(() => {
    const timer = window.setTimeout(
      () => useTeamStore.getState().fitMap?.(),
      50,
    );
    return () => window.clearTimeout(timer);
  }, [cardIds]);

  return (
    <div className="relative size-full" aria-label={copy.team.mapLabel}>
      <div ref={surface} className="absolute inset-0" />
      <div
        ref={layer}
        className="pointer-events-none absolute inset-0 overflow-hidden"
      >
        {members.flatMap((member) => {
          if (!placed.has(member.id)) return [];
          return [
            <div
              key={member.id}
              ref={(element) => {
                if (element) cards.current.set(member.id, element);
                else cards.current.delete(member.id);
              }}
              className="pointer-events-auto absolute top-0 left-0 origin-top-left"
            >
              <TeamNodeCard
                member={member}
                hasChildren={members.some(
                  (other) => other.parentId === member.id,
                )}
                expanded={expanded.includes(member.id)}
                selected={member.id === selectedId}
                dimmed={focus !== null && !focus.has(member.id)}
                onOpen={() => {
                  useTeamStore.getState().select(member.id);
                  openMember(member.id);
                }}
              />
            </div>,
          ];
        })}
      </div>
    </div>
  );
}
