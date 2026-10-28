import {
  SegmentedControl,
  type SegmentOption,
} from "@/components/SegmentedControl";
import { teamModeOf, useTeamStore, type TeamMode } from "@/team/team-store";
import { copy } from "@/ui-copy";

const OPTIONS: readonly SegmentOption<TeamMode>[] = [
  { value: "terminal", label: copy.team.mode.terminal },
  { value: "team", label: copy.team.mode.team },
];

/** Terminal | Team, in the sidebar header. */
export function TeamModeSwitch({ path }: { path: string | null }) {
  const mode = useTeamStore((state) => teamModeOf(state.modeByWorktree, path));
  if (!path) return null;

  return (
    <SegmentedControl
      label={copy.team.modeLabel}
      options={OPTIONS}
      value={mode}
      onChange={(next) => useTeamStore.getState().setMode(path, next)}
    />
  );
}
