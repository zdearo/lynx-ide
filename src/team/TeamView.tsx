import { useEffect } from "react";
import { toast } from "sonner";

import { useBotsStore } from "@/bots/bots-store";
import { Button } from "@/components/ui/button";
import { errorMessage } from "@/lib/error-message";
import { TeamAddDialog } from "@/team/TeamAddDialog";
import { TeamMap } from "@/team/TeamMap";
import { TeamNodeSheet } from "@/team/TeamNodeSheet";
import { TeamTerminalDialog } from "@/team/TeamTerminalDialog";
import { useTeamMembers, useTeamStore } from "@/team/team-store";
import { copy } from "@/ui-copy";
import { useWorkspaceStore } from "@/workspace/workspace-store";

/**
 * The team of the worktree on the sidebar, as an org chart. The picked member opens in a panel over the map; Escape
 * clears the pick. `area` is the element the terminal dialog covers.
 */
export function TeamView({ area }: { area: HTMLElement | null }) {
  const members = useTeamMembers();
  const worktree = useWorkspaceStore((state) => state.selectedWorktreePath);
  const loaded = useTeamStore((state) => state.loaded);
  const expanded = useTeamStore((state) => state.expanded);
  const selectedId = useTeamStore((state) => state.selectedId);
  const pairs = useTeamStore((state) => state.pairs);
  const extraRows = useTeamStore((state) => state.extraRows);
  const siblingOrder = useTeamStore((state) => state.siblingOrder);
  const selected = members.find((member) => member.id === selectedId);
  const loadBots = useBotsStore((state) => state.load);

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
        (members.length > 0 ? (
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
          <TeamEmpty hasWorktree={worktree !== null} />
        ))}
      <TeamTerminalDialog container={area} />
      <TeamAddDialog />
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
