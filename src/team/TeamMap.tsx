import "@xyflow/react/dist/style.css";

import {
  ConnectionMode,
  ReactFlow,
  type Edge,
  type EdgeChange,
  type NodeChange,
} from "@xyflow/react";
import { useMemo, useState, type CSSProperties } from "react";

import type { TeamMember } from "@/team/team-fixtures";
import {
  extraRowsAt,
  siblingOrderAt,
  teamLayout,
  type SiblingOrder,
  TEAM_CARD_HEIGHT,
  TEAM_CARD_WIDTH,
  type TeamPosition,
} from "@/team/team-layout";
import { canOrchestrate, linkOf } from "@/team/team-hierarchy";
import {
  isValidConnection,
  pairKey,
  partnersOf,
  type TeamPair,
} from "@/team/team-pairs";
import { TeamEdge } from "@/team/TeamEdge";
import { TeamGhostCard, type TeamGhostNode } from "@/team/TeamGhostCard";
import { orchestrationRoute, pairRoute, roundedPath } from "@/team/team-routes";
import { TeamNodeCard, type TeamCardNode } from "@/team/TeamNodeCard";
import { openMember, useTeamStore } from "@/team/team-store";
import { useGlidedPositions } from "@/team/use-glided-positions";
import { copy } from "@/ui-copy";

/**
 * The size React Flow counts as measured. The nodes are rebuilt on every frame of a drag or a glide, and a
 * node without it is not "ready" for a moment, which makes React Flow drop and redraw every line.
 */
const CARD_SIZE = { width: TEAM_CARD_WIDTH, height: TEAM_CARD_HEIGHT };

// Defined once: React Flow would remount every card if this object were rebuilt.
const nodeTypes = { member: TeamNodeCard, ghost: TeamGhostCard };
const edgeTypes = { team: TeamEdge };

/** The selected member, the members above it, and its partners: what stays in focus. Null with no selection. */
function focusOf(
  members: readonly TeamMember[],
  pairs: readonly TeamPair[],
  selectedId: string | null,
): Set<string> | null {
  const selected = members.find((member) => member.id === selectedId);
  if (!selected) return null;
  const focus = new Set([selected.id]);
  let parentId = selected.parentId;
  while (parentId && !focus.has(parentId)) {
    focus.add(parentId);
    parentId =
      members.find((member) => member.id === parentId)?.parentId ?? null;
  }
  for (const id of partnersOf(pairs, selected.id)) focus.add(id);
  return focus;
}

function lineStyle(
  from: string,
  to: string,
  focus: Set<string> | null,
  dashed: boolean,
): CSSProperties {
  const inFocus = focus !== null && focus.has(from) && focus.has(to);
  // A solid line orchestrates, a dotted one only talks.
  return {
    stroke: inFocus ? "var(--foreground)" : "var(--muted-foreground)",
    strokeWidth: 1.5,
    strokeDasharray: dashed ? "1 6" : undefined,
    strokeLinecap: dashed ? "round" : undefined,
    opacity: focus && !inFocus ? 0.25 : 1,
  };
}

type TeamMapNode = TeamCardNode | TeamGhostNode;

/**
 * The team as an org chart. Cards are placed by teamLayout. A card can be taken anywhere with the mouse:
 * a dashed ghost shows the row it would take under its parent, and the other cards already make room for
 * it (and return if it leaves); on release they glide to their final places (the card's own members follow
 * it). Holding a card does not select it, only a click does. A line from a bottom handle to a top handle
 * makes the lower card answer to the upper one (an orchestrator); the side handles draw a pair, a dashed line.
 * The Delete key removes a picked line: a pair, or an orchestration, which leaves the member answering to no one.
 */
export function TeamMap({
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
  // The card being held and where the pointer has it.
  const [drag, setDrag] = useState<{
    id: string;
    position: TeamPosition;
  } | null>(null);
  // The pair lines React Flow has selected, so the Delete key can remove them.
  const [picked, setPicked] = useState<ReadonlySet<string>>(() => new Set());

  const base = useMemo(
    () => teamLayout(members, expanded, extraRows, siblingOrder, pairs),
    [members, expanded, extraRows, siblingOrder, pairs],
  );
  // The row the held card is over. While it is not its own, the others already take the places they would
  // have on release, and go back if it returns.
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
  const drawn = useGlidedPositions(targets, drag);
  const focus = useMemo(
    () => focusOf(members, pairs, selectedId),
    [members, pairs, selectedId],
  );

  // Built apart from the positions, which change on every frame of a drag or a glide.
  const cards = useMemo(
    () =>
      members.flatMap((member): TeamCardNode[] =>
        base.has(member.id)
          ? [
              {
                id: member.id,
                type: "member",
                position: { x: 0, y: 0 },
                // The card's size is known, so React Flow never has to measure it (it hides what it has not).
                width: TEAM_CARD_WIDTH,
                height: TEAM_CARD_HEIGHT,
                measured: CARD_SIZE,
                // A member that answers to no one stays on the top row, so it only moves sideways.
                draggable: true,
                // Delete takes a picked pair, never a card: React Flow would also drop the card's pairs.
                deletable: false,
                selected: member.id === selectedId,
                focusable: false,
                data: {
                  member,
                  hasChildren: members.some(
                    (other) => other.parentId === member.id,
                  ),
                  expanded: expanded.includes(member.id),
                  dimmed: focus !== null && !focus.has(member.id),
                },
              },
            ]
          : [],
      ),
    [members, base, expanded, selectedId, focus],
  );

  // Where the held card would land: the row nearest to it, under its parent.
  const spot = targets !== base && dragId ? targets.get(dragId) : undefined;
  const ghost = useMemo((): TeamGhostNode | null => {
    if (!spot) return null;
    return {
      id: "ghost",
      type: "ghost",
      position: spot,
      width: TEAM_CARD_WIDTH,
      height: TEAM_CARD_HEIGHT,
      measured: CARD_SIZE,
      data: {},
      selectable: false,
      draggable: false,
      focusable: false,
      connectable: false,
      deletable: false,
      style: { pointerEvents: "none" },
    };
  }, [spot]);

  const nodes: TeamMapNode[] = cards.map((card) => ({
    ...card,
    position: drawn.get(card.id) ?? targets.get(card.id) ?? card.position,
  }));
  if (ghost) nodes.push(ghost);

  // The lines are routed from where the cards are drawn now, so they follow a drag and a glide.
  const edges = useMemo(() => {
    const spots = new Map<string, TeamPosition>();
    for (const id of base.keys()) {
      const spot = drawn.get(id) ?? targets.get(id);
      if (spot) spots.set(id, spot);
    }
    const around = (...ids: string[]) =>
      [...spots].filter(([id]) => !ids.includes(id)).map(([, spot]) => spot);
    const lines: Edge[] = [];
    for (const member of members) {
      const parent = member.parentId ? spots.get(member.parentId) : undefined;
      const child = spots.get(member.id);
      if (member.parentId && parent && child) {
        const id = `${member.parentId}-${member.id}`;
        lines.push({
          id,
          source: member.parentId,
          sourceHandle: "bottom",
          target: member.id,
          targetHandle: "top",
          type: "team",
          data: {
            path: roundedPath(
              orchestrationRoute(
                parent,
                child,
                around(member.parentId, member.id),
              ),
            ),
          },
          selected: picked.has(id),
          style: lineStyle(member.parentId, member.id, focus, false),
        });
      }
    }
    for (const pair of pairs) {
      const from = spots.get(pair.from);
      const to = spots.get(pair.to);
      if (!from || !to) continue;
      // The line leaves the left card from its right side and enters the right card from its left side.
      const [left, right] =
        from.x <= to.x ? [pair.from, pair.to] : [pair.to, pair.from];
      const key = pairKey(pair.from, pair.to);
      lines.push({
        id: key,
        source: left,
        sourceHandle: "right",
        target: right,
        targetHandle: "left",
        type: "team",
        data: {
          path: roundedPath(pairRoute(from, to, around(pair.from, pair.to))),
        },
        selected: picked.has(key),
        style: lineStyle(pair.from, pair.to, focus, true),
      });
    }
    return lines;
  }, [members, base, drawn, targets, pairs, picked, focus]);

  // Only a drag matters here: the rest of the change is the layout's job, not React Flow's.
  const onNodesChange = (changes: NodeChange[]) => {
    for (const change of changes) {
      if (change.type === "position" && change.dragging && change.position) {
        setDrag({ id: change.id, position: change.position });
      }
    }
  };
  const onEdgesChange = (changes: EdgeChange[]) => {
    setPicked((current) => {
      const next = new Set(current);
      for (const change of changes) {
        if (change.type === "select") {
          if (change.selected) next.add(change.id);
          else next.delete(change.id);
        } else if (change.type === "remove") {
          next.delete(change.id);
        }
      }
      return next;
    });
  };

  return (
    <ReactFlow
      className="team-map"
      aria-label={copy.team.mapLabel}
      nodes={nodes}
      edges={edges}
      nodeTypes={nodeTypes}
      edgeTypes={edgeTypes}
      fitView
      minZoom={0.4}
      maxZoom={1.5}
      nodeDragThreshold={4}
      nodesConnectable
      connectionMode={ConnectionMode.Loose}
      isValidConnection={(connection) => {
        const link = linkOf(connection);
        if (link?.kind === "orchestrates") {
          return canOrchestrate(members, link.parent, link.child);
        }
        return link
          ? isValidConnection(
              { source: link.from, target: link.to },
              members,
              pairs,
            )
          : false;
      }}
      onConnect={(connection) => {
        const link = linkOf(connection);
        const store = useTeamStore.getState();
        if (link?.kind === "orchestrates")
          store.setParent(link.child, link.parent);
        else if (link) store.setLinked(link.from, link.to, true);
      }}
      deleteKeyCode={["Backspace", "Delete"]}
      onEdgesDelete={(deleted) => {
        for (const edge of deleted) {
          // A tree line leaves from the bottom handle: taking it away leaves the member answering to no one.
          if (edge.sourceHandle === "bottom") {
            useTeamStore.getState().setParent(edge.target, null);
          } else {
            useTeamStore.getState().setLinked(edge.source, edge.target, false);
          }
        }
      }}
      edgesReconnectable={false}
      elementsSelectable
      panOnDrag
      zoomOnScroll
      onNodesChange={onNodesChange}
      onEdgesChange={onEdgesChange}
      onNodeDragStop={(_, node) => {
        setDrag(null);
        useTeamStore
          .getState()
          .setExtraRows(
            node.id,
            extraRowsAt(members, base, node.id, node.position.y),
          );
        if (order !== siblingOrder) {
          useTeamStore.getState().setSiblingOrder(order);
        }
      }}
      onInit={(flow) =>
        useTeamStore.setState({
          fitMap: () => void flow.fitView({ padding: 0.2, duration: 200 }),
        })
      }
      onNodeClick={(_, node) => useTeamStore.getState().select(node.id)}
      onNodeDoubleClick={(_, node) => {
        useTeamStore.getState().select(node.id);
        openMember(node.id);
      }}
      onPaneClick={() => useTeamStore.getState().select(null)}
    />
  );
}
