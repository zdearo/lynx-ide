import { useEffect } from "react";
import { toast } from "sonner";

import { useBotsStore } from "@/bots/bots-store";
import { Button } from "@/components/ui/button";
import { errorMessage } from "@/lib/error-message";
import { TeamAddDialog } from "@/team/TeamAddDialog";
import { TeamDeleteRunDialog } from "@/team/TeamDeleteRunDialog";
import { TeamRunDialog } from "@/team/TeamRunDialog";
import { TeamMap } from "@/team/TeamMap";
import { TeamNodeSheet } from "@/team/TeamNodeSheet";
import { TeamTerminalDialog } from "@/team/TeamTerminalDialog";
import { useRunMembers, useTeamStore, watchTeam } from "@/team/team-store";
import { copy } from "@/ui-copy";

/**
 * The team in the central area: the org chart, or the tree when it is chosen or the window is narrow.
 * The picked member opens in a panel over either; Escape clears the pick. `area` is the element the
 * terminal dialog covers.
 */
export function TeamView({ area }: { area: HTMLElement | null }) {
  const members = useRunMembers();
  const loaded = useTeamStore((state) => state.loaded);
  const hasRuns = useTeamStore((state) => state.runs.length > 0);
  const expanded = useTeamStore((state) => state.expanded);
  const selectedId = useTeamStore((state) => state.selectedId);
  const pairs = useTeamStore((state) => state.pairs);
  const extraRows = useTeamStore((state) => state.extraRows);
  const siblingOrder = useTeamStore((state) => state.siblingOrder);
  const selected = members.find((member) => member.id === selectedId);
  const loadBots = useBotsStore((state) => state.load);

  // The team is read when the view opens, and again each time the backend changes it.
  useEffect(() => watchTeam(), []);

  // The bot members draw their avatar and engine from the Bots list, which the Bots view may not have loaded.
  useEffect(() => {
    loadBots().catch((error: unknown) =>
      toast.error(copy.team.botsLoadFailed, {
        description: errorMessage(error),
      }),
    );
  }, [loadBots]);

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
      {loaded &&
        (hasRuns ? (
          <>
            <TeamMap
              members={members}
              expanded={expanded}
              selectedId={selectedId}
              extraRows={extraRows}
              siblingOrder={siblingOrder}
              pairs={pairs}
            />
            {selected && (
              <TeamNodeSheet
                key={selected.id}
                member={selected}
                members={members}
              />
            )}
          </>
        ) : (
          <TeamEmpty />
        ))}
      <TeamTerminalDialog container={area} />
      <TeamAddDialog />
      <TeamRunDialog />
      <TeamDeleteRunDialog />
    </div>
  );
}

/** The team with no run yet: a line about what a run is, and the action that starts one. */
function TeamEmpty() {
  return (
    <div className="flex h-full flex-col items-center justify-center gap-3 p-6 text-center">
      <div className="grid max-w-sm gap-1">
        <h2 className="text-ui font-medium">{copy.team.empty.title}</h2>
        <p className="text-nested text-muted-foreground">
          {copy.team.empty.description}
        </p>
      </div>
      <Button
        size="sm"
        onClick={() => useTeamStore.getState().setNewRunOpen(true)}
      >
        {copy.team.runs.newRun}
      </Button>
    </div>
  );
}
