import { memo, useEffect } from "react";

import { toast } from "sonner";

import { useBotsStore } from "@/bots/bots-store";
import { Button } from "@/components/ui/button";
import { errorMessage } from "@/lib/error-message";
import { TeamAddDialog } from "@/team/TeamAddDialog";
import { TeamCanvas } from "@/team/TeamCanvas";
import { TeamNotes } from "@/team/TeamNotes";
import {
  restoreOpenTerminals,
  useTeamMembersOf,
  useTeamStore,
} from "@/team/team-store";
import { copy } from "@/ui-copy";

/**
 * The team of one worktree, as an org chart. Escape clears the pick, except in a card's terminal: xterm cancels the keys
 * it sends to the agent, so Escape never reaches this view there. A team that is not on the screen stays mounted,
 * hidden, so coming back to it keeps its map and its open terminals as they were.
 */
export const TeamView = memo(TeamViewBase);

function TeamViewBase({
  worktree,
  selected,
}: {
  worktree: string;
  /** Whether this is the worktree on the sidebar, the one the screen shows. */
  selected: boolean;
}) {
  const members = useTeamMembersOf(worktree);
  const loaded = useTeamStore((state) => state.loaded);
  const expanded = useTeamStore((state) => state.expanded);
  const selectedId = useTeamStore((state) => state.selectedId);
  const pairs = useTeamStore((state) => state.pairs);
  const extraRows = useTeamStore((state) => state.extraRows);
  const siblingOrder = useTeamStore((state) => state.siblingOrder);
  const loadBots = useBotsStore((state) => state.load);

  // The terminals saved as open reopen once the team is loaded, a few per frame.
  useEffect(() => {
    if (loaded) restoreOpenTerminals();
  }, [loaded]);

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
      className="relative flex h-full flex-col"
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
      <div className="relative min-h-0 flex-1">
        {loaded &&
          (members.length > 0 ? (
            <TeamCanvas
              members={members}
              expanded={expanded}
              selectedId={selectedId}
              extraRows={extraRows}
              siblingOrder={siblingOrder}
              pairs={pairs}
              selected={selected}
            />
          ) : (
            <TeamEmpty hasWorktree />
          ))}
      </div>
      {loaded && selected && <TeamNotes />}
      {selected && <TeamAddDialog />}
    </div>
  );
}

/** The team with no member to show: a worktree to pick, or a worktree with no agent yet and the action that adds one. */
function TeamEmpty({ hasWorktree }: { hasWorktree: boolean }) {
  return (
    <div className="flex h-full flex-col items-center justify-center gap-3 p-6 text-center">
      {hasWorktree ? (
        <>
          <div className="grid max-w-sm gap-1">
            <h2 className="text-ui font-medium">{copy.team.empty.noMembers}</h2>
            <p className="text-nested text-muted-foreground">
              {copy.team.empty.noMembersDescription}
            </p>
          </div>
          <Button
            size="sm"
            onClick={() => useTeamStore.getState().setAddOpen(true)}
          >
            {copy.team.recruit}
          </Button>
        </>
      ) : (
        <h2 className="text-ui font-medium">{copy.team.empty.noWorktree}</h2>
      )}
    </div>
  );
}
