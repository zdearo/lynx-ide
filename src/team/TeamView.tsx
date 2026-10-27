import { ScrollArea } from "@/components/ui/scroll-area";
import { TEAM_MAP_QUERY } from "@/hooks/layout-queries";
import { useMediaQuery } from "@/hooks/use-media-query";
import { TeamMap } from "@/team/TeamMap";
import { TeamNodeSheet } from "@/team/TeamNodeSheet";
import { TeamTerminalDialog } from "@/team/TeamTerminalDialog";
import { teamRows } from "@/team/team-rows";
import { TeamTree } from "@/team/TeamTree";
import { useTeamStore } from "@/team/team-store";

/**
 * The team in the central area: the org chart, or the tree when it is chosen or the window is narrow.
 * The picked member opens in a panel over either; Escape clears the pick. `area` is the element the
 * terminal dialog covers.
 */
export function TeamView({ area }: { area: HTMLElement | null }) {
  const members = useTeamStore((state) => state.members);
  const expanded = useTeamStore((state) => state.expanded);
  const selectedId = useTeamStore((state) => state.selectedId);
  const chosen = useTeamStore((state) => state.layout);
  const wide = useMediaQuery(TEAM_MAP_QUERY);
  const selected = members.find((member) => member.id === selectedId);

  return (
    <div
      className="relative h-full"
      onKeyDown={(event) => {
        // Keys from the dialog and the menus come through the portals, which are not inside here.
        if (
          event.key === "Escape" &&
          event.target instanceof Node &&
          event.currentTarget.contains(event.target)
        ) {
          useTeamStore.getState().select(null);
        }
      }}
    >
      {wide && chosen === "map" ? (
        <TeamMap
          members={members}
          expanded={expanded}
          selectedId={selectedId}
        />
      ) : (
        <ScrollArea className="h-full">
          <TeamTree
            rows={teamRows(members, expanded)}
            members={members}
            selectedId={selectedId}
          />
        </ScrollArea>
      )}
      {selected && (
        <TeamNodeSheet key={selected.id} member={selected} members={members} />
      )}
      <TeamTerminalDialog container={area} />
    </div>
  );
}
