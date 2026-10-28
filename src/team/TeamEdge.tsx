import { BaseEdge, type Edge, type EdgeProps } from "@xyflow/react";

export type TeamEdgeData = { path: string };

/** A line whose path TeamMap has already routed around the cards. */
export function TeamEdge({
  id,
  data,
  style,
  interactionWidth,
}: EdgeProps<Edge<TeamEdgeData, "team">>) {
  return (
    <BaseEdge
      id={id}
      path={data?.path ?? ""}
      style={style}
      interactionWidth={interactionWidth}
    />
  );
}
