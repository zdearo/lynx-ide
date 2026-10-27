import { create } from "zustand";

import type { HarnessId } from "@/bindings";
import { TERMINAL_PRESETS } from "@/agents/agent-presets";
import {
  TEAM_MEMBERS,
  TEAM_NEEDS_YOU_ID,
  TEAM_ROOT_ID,
  type TeamMember,
} from "@/team/team-fixtures";
import { copy } from "@/ui-copy";

/** A worktree's central area: its terminals, or the team. */
export type TeamMode = "terminal" | "team";
/** How the team is drawn: as the org chart, or as the tree. */
export type TeamLayout = "map" | "list";

interface TeamState {
  members: TeamMember[];
  /** The members whose own members are listed under them. */
  expanded: string[];
  selectedId: string | null;
  modeByWorktree: Record<string, TeamMode>;
  layout: TeamLayout;
  /** Fits the drawn map on screen. Set while the map is on screen, null otherwise. */
  fitMap: (() => void) | null;
  /** The member whose terminal the dialog shows. Kept after the dialog closes, so focus can go back to its card. */
  terminalFor: string | null;
  terminalOpen: boolean;

  setMode: (path: string, mode: TeamMode) => void;
  openTerminal: (id: string) => void;
  closeTerminal: () => void;
  setLayout: (layout: TeamLayout) => void;
  select: (id: string | null) => void;
  toggle: (id: string) => void;
  setRole: (id: string, role: string) => void;
  dismiss: (id: string) => void;
  /** Answers a member that needs the user; it goes back to work. */
  reply: (id: string, text: string) => void;
}

/** A member's role by id, or undefined once it is gone from the run. */
export function roleOf(
  members: readonly TeamMember[],
  id: string | null,
): string | undefined {
  return members.find((member) => member.id === id)?.role;
}

/** The agent's name as the presets give it, e.g. "Claude Code". */
export function agentNameOf(harness: HarnessId): string {
  return (
    TERMINAL_PRESETS.find((preset) => preset.harness === harness)?.label ??
    harness
  );
}

/** The mode of a worktree; a worktree nobody has switched shows its terminals. */
export function teamModeOf(
  modeByWorktree: Record<string, TeamMode>,
  path: string | null,
): TeamMode {
  return path ? (modeByWorktree[path] ?? "terminal") : "terminal";
}

function changeMember(
  members: TeamMember[],
  id: string,
  change: (member: TeamMember) => TeamMember,
): TeamMember[] {
  return members.map((member) => (member.id === id ? change(member) : member));
}

/** The mock run. Every change here is local: nothing reaches a backend. */
export const useTeamStore = create<TeamState>((set) => ({
  members: [...TEAM_MEMBERS],
  expanded: [TEAM_ROOT_ID],
  selectedId: TEAM_NEEDS_YOU_ID,
  modeByWorktree: {},
  layout: "map",
  fitMap: null,
  terminalFor: null,
  terminalOpen: false,

  setMode: (path, mode) =>
    set((state) => ({
      modeByWorktree: { ...state.modeByWorktree, [path]: mode },
      // Leaving the team (or coming back to it) closes its terminal dialog.
      terminalOpen: false,
    })),
  openTerminal: (id) => set({ terminalFor: id, terminalOpen: true }),
  closeTerminal: () => set({ terminalOpen: false }),
  setLayout: (layout) => set({ layout }),
  select: (selectedId) => set({ selectedId }),
  toggle: (id) =>
    set((state) => ({
      expanded: state.expanded.includes(id)
        ? state.expanded.filter((open) => open !== id)
        : [...state.expanded, id],
    })),
  setRole: (id, role) =>
    set((state) => ({
      members: changeMember(state.members, id, (member) => ({
        ...member,
        role,
      })),
    })),
  dismiss: (id) =>
    set((state) => ({
      members: state.members.filter((member) => member.id !== id),
      selectedId: state.selectedId === id ? null : state.selectedId,
    })),
  reply: (id, text) =>
    set((state) => ({
      members: changeMember(state.members, id, (member) => ({
        ...member,
        status: "working",
        question: null,
        output: [...member.output, copy.team.replyLine(text)],
      })),
    })),
}));
