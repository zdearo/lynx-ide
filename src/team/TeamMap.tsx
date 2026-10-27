import "@xyflow/react/dist/style.css";

import { ReactFlow, type Edge } from "@xyflow/react";
import type { CSSProperties } from "react";

import type { TeamMember } from "@/team/team-fixtures";
import { teamLayout } from "@/team/team-layout";
import { TeamNodeCard, type TeamCardNode } from "@/team/TeamNodeCard";
import { useTeamStore } from "@/team/team-store";
import { copy } from "@/ui-copy";

// Defined once: React Flow would remount every card if this object were rebuilt.
const nodeTypes = { member: TeamNodeCard };

/** The selected member, the members above it, and its pair: what stays in focus. Null with no selection. */
function focusOf(
  members: readonly TeamMember[],
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
  for (const id of [
    selected.pairedWith,
    ...members
      .filter((member) => member.pairedWith === selected.id)
      .map((member) => member.id),
  ]) {
    if (id) focus.add(id);
  }
  return focus;
}

function lineStyle(
  from: string,
  to: string,
  focus: Set<string> | null,
  dashed: boolean,
): CSSProperties {
  const inFocus = focus !== null && focus.has(from) && focus.has(to);
  return {
    stroke: inFocus ? "var(--foreground)" : "var(--muted-foreground)",
    strokeWidth: 1.25,
    strokeDasharray: dashed ? "4 4" : undefined,
    opacity: focus && !inFocus ? 0.25 : 1,
  };
}

/** The team as an org chart. Cards and lines are placed by teamLayout; nothing can be dragged or wired. */
export function TeamMap({
  members,
  expanded,
  selectedId,
}: {
  members: readonly TeamMember[];
  expanded: readonly string[];
  selectedId: string | null;
}) {
  const positions = teamLayout(members, expanded);
  const focus = focusOf(members, selectedId);

  const nodes = members.flatMap((member): TeamCardNode[] => {
    const position = positions.get(member.id);
    if (!position) return [];
    return [
      {
        id: member.id,
        type: "member",
        position,
        selected: member.id === selectedId,
        focusable: false,
        data: {
          member,
          hasChildren: members.some((other) => other.parentId === member.id),
          expanded: expanded.includes(member.id),
          dimmed: focus !== null && !focus.has(member.id),
        },
      },
    ];
  });

  const shown = new Set(nodes.map((node) => node.id));
  const edges: Edge[] = [];
  const pairs = new Set<string>();
  for (const member of members) {
    if (!shown.has(member.id)) continue;
    if (member.parentId && shown.has(member.parentId)) {
      edges.push({
        id: `${member.parentId}-${member.id}`,
        source: member.parentId,
        sourceHandle: "bottom",
        target: member.id,
        targetHandle: "top",
        type: "smoothstep",
        style: lineStyle(member.parentId, member.id, focus, false),
      });
    }
    // A pair is one line, however the two members list each other.
    const other = member.pairedWith;
    const key = other ? [member.id, other].sort().join("|") : "";
    if (other && shown.has(other) && !pairs.has(key)) {
      pairs.add(key);
      edges.push({
        id: key,
        source: member.id,
        sourceHandle: "bottom",
        target: other,
        targetHandle: "bottom-in",
        type: "smoothstep",
        label: copy.team.pairLabel,
        labelStyle: {
          fill: "var(--muted-foreground)",
          fontSize: "var(--text-meta)",
        },
        labelBgStyle: { fill: "var(--background)" },
        labelBgPadding: [6, 3],
        labelBgBorderRadius: 4,
        style: lineStyle(member.id, other, focus, true),
      });
    }
  }

  return (
    <ReactFlow
      className="team-map"
      aria-label={copy.team.mapLabel}
      nodes={nodes}
      edges={edges}
      nodeTypes={nodeTypes}
      fitView
      minZoom={0.4}
      maxZoom={1.5}
      nodesDraggable={false}
      nodesConnectable={false}
      edgesReconnectable={false}
      elementsSelectable
      panOnDrag
      zoomOnScroll
      onInit={(flow) =>
        useTeamStore.setState({
          fitMap: () => void flow.fitView({ padding: 0.2, duration: 200 }),
        })
      }
      onNodeClick={(_, node) => useTeamStore.getState().select(node.id)}
      onNodeDoubleClick={(_, node) => {
        useTeamStore.getState().select(node.id);
        useTeamStore.getState().openTerminal(node.id);
      }}
      onPaneClick={() => useTeamStore.getState().select(null)}
    />
  );
}
