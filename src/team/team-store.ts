import { useMemo } from "react";
import { toast } from "sonner";
import { create } from "zustand";

import type {
  BotSummary,
  HarnessId,
  NewTeamMember,
  TeamMember as TeamMemberRecord,
  TeamRun,
  TeamSnapshot,
} from "@/bindings";
import { TERMINAL_PRESETS } from "@/agents/agent-presets";
import { type BotProvider } from "@/bots/bot-colors";
import { isMockBackend } from "@/dev/mock-mode";
import { errorMessage } from "@/lib/error-message";
import {
  TEAM_DOCS_MEMBERS,
  TEAM_MEMBERS,
  TEAM_NEEDS_YOU_ID,
  TEAM_ROOT_ID,
  type TeamMember,
  type TeamMemberBase,
} from "@/team/team-fixtures";
import { teamBackend, onTeamChanged } from "@/team/team-backend";
import type { SiblingOrder } from "@/team/team-layout";
import type { TeamPair } from "@/team/team-pairs";
import { copy } from "@/ui-copy";

/** A worktree's central area: its terminals, or the team. */
export type TeamMode = "terminal" | "team";

/** The simulated live fields of a member. The backend does not keep them yet. */
type TeamLive = Pick<
  TeamMemberBase,
  | "status"
  | "activity"
  | "branch"
  | "added"
  | "removed"
  | "waitingOn"
  | "note"
  | "question"
  | "output"
>;

const IDLE: TeamLive = {
  status: "idle",
  activity: { text: "", at: 0 },
  branch: null,
  added: 0,
  removed: 0,
  waitingOn: null,
  note: null,
  question: null,
  output: [],
};

/** The live fields by member id. Only the mock has them, seeded from its fixtures; the real app starts every member idle. */
const LIVE: Record<string, TeamLive> = isMockBackend()
  ? Object.fromEntries(
      [...TEAM_MEMBERS, ...TEAM_DOCS_MEMBERS].map((member) => [
        member.id,
        {
          status: member.status,
          activity: member.activity,
          branch: member.branch,
          added: member.added,
          removed: member.removed,
          waitingOn: member.waitingOn,
          note: member.note,
          question: member.question,
          output: member.output,
        },
      ]),
    )
  : {};

interface TeamState {
  /** Whether the backend has sent the team once. The screen waits for it, so an empty team is not shown first. */
  loaded: boolean;
  runs: TeamRun[];
  members: TeamMember[];
  pairs: TeamPair[];
  /** The run on screen: the map, the summary and the menus show only its members. Null when there is no run. */
  runId: string | null;
  newRunOpen: boolean;
  /** Whether the confirmation for deleting the run on screen is open. */
  deleteRunOpen: boolean;
  /** The members whose own members are listed under them. Kept per run in storage. */
  expanded: string[];
  selectedId: string | null;
  /** How many rows each member sits below its default row, by id. */
  extraRows: Record<string, number>;
  /** The left-to-right order of each member's own members, once the user has moved one sideways. */
  siblingOrder: SiblingOrder;
  modeByWorktree: Record<string, TeamMode>;
  /** Fits the drawn map on screen. Set while the map is on screen, null otherwise. */
  fitMap: (() => void) | null;
  /** The member whose terminal the dialog shows. Kept after the dialog closes, so focus can go back to its card. */
  terminalFor: string | null;
  terminalOpen: boolean;
  /** Whether the add-member dialog is open. */
  addOpen: boolean;

  /** Reads the team from the backend. A read that a newer one overtakes is dropped. */
  load: () => Promise<void>;
  setMode: (path: string, mode: TeamMode) => void;
  openTerminal: (id: string) => void;
  closeTerminal: () => void;
  select: (id: string | null) => void;
  toggle: (id: string) => void;
  /** The changes below change the backend and show nothing until its event brings the team again. A refused one changes nothing. */
  setRole: (id: string, role: string) => Promise<void>;
  dismiss: (id: string) => Promise<void>;
  setLinked: (from: string, to: string, linked: boolean) => Promise<void>;
  setOrchestrator: (id: string, on: boolean) => Promise<void>;
  /** Puts a member under an orchestrator, or under no one (null). */
  setParent: (id: string, parentId: string | null) => Promise<void>;
  setExtraRows: (id: string, rows: number) => void;
  setSiblingOrder: (order: SiblingOrder) => void;
  setAddOpen: (open: boolean) => void;
  setNewRunOpen: (open: boolean) => void;
  setDeleteRunOpen: (open: boolean) => void;
  setRun: (id: string) => void;
  /** Starts a run with its own goal and an orchestrator at the top, and shows it. False when it was refused. */
  addRun: (title: string) => Promise<boolean>;
  /** Deletes a run with its members and lines, and closes the confirmation once it is gone. */
  deleteRun: (runId: string) => Promise<void>;
  /** Changes a member's task and done-when. False when the backend refused it, and the sheet keeps what was typed. */
  setTask: (id: string, task: string, acceptance: string) => Promise<boolean>;
  /** Adds a member and picks it. False when it was refused, and the dialog keeps what was typed. */
  addMember: (input: NewTeamMember) => Promise<boolean>;
}

/** A member the backend lists, with its live fields. One that lacks the agent or bot its kind names is refused, not guessed. */
function memberOf(record: TeamMemberRecord): TeamMember {
  const base = {
    id: record.id,
    runId: record.runId,
    role: record.role,
    parentId: record.parentId,
    orchestrator: record.orchestrator,
    task: record.task,
    acceptance: record.acceptance,
    ...(LIVE[record.id] ?? IDLE),
  };
  if (record.kind === "bot" && record.botId) {
    return { ...base, kind: "bot", botId: record.botId };
  }
  if (record.kind === "agent" && record.harness) {
    return { ...base, kind: "agent", harness: record.harness };
  }
  throw new Error(`team member ${record.id} is missing its ${record.kind}`);
}

/** Runs a backend change. Its value, or null when the backend refused it: then the message is shown and nothing changes. */
async function change<T>(run: () => Promise<T>): Promise<{ value: T } | null> {
  try {
    return { value: await run() };
  } catch (error) {
    toast.error(copy.team.changeRefused, { description: errorMessage(error) });
    return null;
  }
}

/** The bot a bot member is, from the Bots list. Undefined while the list is not loaded, or the bot is gone. */
export function botOf(
  member: TeamMember,
  bots: readonly BotSummary[] | null,
): BotSummary | undefined {
  return member.kind === "bot"
    ? bots?.find((bot) => bot.id === member.botId)
    : undefined;
}

/** A member's second line on its card: the agent's name, or a bot's provider and model. */
export function engineOf(
  member: TeamMember,
  bots: readonly BotSummary[] | null,
): string {
  if (member.kind === "agent") return agentNameOf(member.harness);
  const bot = botOf(member, bots);
  if (!bot) return "";
  const provider =
    copy.bots.providers[bot.provider as BotProvider] ?? bot.provider;
  return copy.team.engine(provider, bot.model);
}

/** Opens a member in the dialog over the central area: its terminal, or a bot's chat. The card, the sheet and the menus use it. */
export function openMember(id: string) {
  useTeamStore.getState().openTerminal(id);
}

/** The members of one run. */
export function membersOfRun(
  members: readonly TeamMember[],
  runId: string | null,
): TeamMember[] {
  return members.filter((member) => member.runId === runId);
}

/** The members of the run on screen. */
export function useRunMembers(): TeamMember[] {
  const members = useTeamStore((state) => state.members);
  const runId = useTeamStore((state) => state.runId);
  return useMemo(() => membersOfRun(members, runId), [members, runId]);
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

/** Where the expansion is kept: the expanded members of each run, in one object. */
const EXPANDED_KEY = "lynx.team.expanded";

/** The expanded members of each run. A run with no entry has not been seen yet. */
type StoredExpanded = Record<string, string[]>;

/** Reads the stored expansion. Storage can be blocked or hold anything, and then nothing is remembered. */
function readStoredExpanded(): StoredExpanded {
  try {
    const stored: unknown = JSON.parse(
      localStorage.getItem(EXPANDED_KEY) ?? "{}",
    );
    if (!stored || typeof stored !== "object" || Array.isArray(stored)) {
      return {};
    }
    return Object.fromEntries(
      Object.entries(stored).filter(
        (entry): entry is [string, string[]] =>
          Array.isArray(entry[1]) &&
          entry[1].every((id) => typeof id === "string"),
      ),
    );
  } catch {
    return {};
  }
}

function writeStoredExpanded(stored: StoredExpanded) {
  try {
    localStorage.setItem(EXPANDED_KEY, JSON.stringify(stored));
  } catch {
    // Not remembering is fine.
  }
}

/** The expansion as stored: read once when the store is made, then kept in step with each write. */
let storedExpanded = readStoredExpanded();

/** Expanded when the store is made: what was stored, else the mock's seeded view. */
function expandedAtStart(): string[] {
  return Object.keys(storedExpanded).length > 0
    ? [...new Set(Object.values(storedExpanded).flat())]
    : isMockBackend()
      ? [TEAM_ROOT_ID, "docs-lead"]
      : [];
}

/** The team the backend has sent, with the run on screen kept when it still exists, else the first run. */
function teamOf(
  snapshot: TeamSnapshot,
  state: Pick<TeamState, "runId" | "selectedId" | "expanded">,
): Partial<TeamState> {
  const kept = snapshot.runs.some((run) => run.id === state.runId);
  // A run that disappeared takes its picked member with it; the first start has none picked yet.
  const lost = state.runId !== null && !kept;
  // The top orchestrator of a run opens the first time the run is seen, so its members show. After that the
  // user's choice is stored, and a reload keeps it.
  const fresh = snapshot.members
    .filter(
      (member) =>
        member.orchestrator &&
        member.parentId === null &&
        !Object.hasOwn(storedExpanded, member.runId),
    )
    .map((member) => member.id);
  return {
    expanded: [...new Set([...state.expanded, ...fresh])],
    loaded: true,
    runs: snapshot.runs,
    members: snapshot.members.map(memberOf),
    pairs: snapshot.pairs.map(({ a, b }) => ({ from: a, to: b })),
    runId: kept ? state.runId : (snapshot.runs[0]?.id ?? null),
    selectedId: lost ? null : state.selectedId,
  };
}

/** The newest read of the team. Only it may write, so a slow earlier read cannot overwrite it. */
let latestLoad = 0;

/** The team on screen. Its runs, members and lines come from the backend; the rest is the screen's own state. */
export const useTeamStore = create<TeamState>((set, get) => ({
  loaded: false,
  runs: [],
  members: [],
  pairs: [],
  runId: null,
  newRunOpen: false,
  deleteRunOpen: false,
  // The mock opens on its seeded view when nothing is stored; the real app starts with nothing picked or unfolded.
  expanded: expandedAtStart(),
  selectedId: isMockBackend() ? TEAM_NEEDS_YOU_ID : null,
  extraRows: {},
  siblingOrder: {},
  modeByWorktree: {},
  fitMap: null,
  terminalFor: null,
  terminalOpen: false,
  addOpen: false,

  load: async () => {
    const sequence = ++latestLoad;
    try {
      const snapshot = await teamBackend.snapshot();
      if (sequence === latestLoad) {
        set((state) => teamOf(snapshot, state));
      }
    } catch (error) {
      toast.error(copy.team.loadFailed, { description: errorMessage(error) });
    }
  },
  setMode: (path, mode) =>
    set((state) => ({
      modeByWorktree: { ...state.modeByWorktree, [path]: mode },
      // Leaving the team (or coming back to it) closes its terminal dialog.
      terminalOpen: false,
    })),
  openTerminal: (id) => set({ terminalFor: id, terminalOpen: true }),
  closeTerminal: () => set({ terminalOpen: false }),
  select: (selectedId) => set({ selectedId }),
  toggle: (id) =>
    set((state) => ({
      expanded: state.expanded.includes(id)
        ? state.expanded.filter((open) => open !== id)
        : [...state.expanded, id],
    })),
  setRole: async (id, role) => {
    await change(() => teamBackend.setRole(id, role));
  },
  setOrchestrator: async (id, on) => {
    await change(() => teamBackend.setOrchestrator(id, on));
  },
  setParent: async (id, parentId) => {
    if (!(await change(() => teamBackend.setParent(id, parentId)))) return;
    // The new parent shows its members, with this one last on its row and at its default level.
    set((state) => ({
      expanded:
        parentId && !state.expanded.includes(parentId)
          ? [...state.expanded, parentId]
          : state.expanded,
      extraRows: { ...state.extraRows, [id]: 0 },
      siblingOrder: Object.fromEntries(
        Object.entries(state.siblingOrder).map(([key, ids]) => [
          key,
          ids.filter((other) => other !== id),
        ]),
      ),
    }));
  },
  dismiss: async (id) => {
    if (!(await change(() => teamBackend.dismissMember(id)))) return;
    set((state) => ({
      selectedId: state.selectedId === id ? null : state.selectedId,
    }));
  },
  setLinked: async (from, to, linked) => {
    await change(() => teamBackend.setLinked(from, to, linked));
  },
  setExtraRows: (id, rows) =>
    set((state) => ({
      extraRows: { ...state.extraRows, [id]: Math.max(0, rows) },
    })),
  setSiblingOrder: (siblingOrder) => set({ siblingOrder }),
  setAddOpen: (addOpen) => set({ addOpen }),
  setNewRunOpen: (newRunOpen) => set({ newRunOpen }),
  setDeleteRunOpen: (deleteRunOpen) => set({ deleteRunOpen }),
  setRun: (runId) => set({ runId, selectedId: null }),
  addRun: async (title) => {
    const done = await change(() => teamBackend.createRun(title));
    if (!done) return false;
    set({ runId: done.value.id, selectedId: null, newRunOpen: false });
    return true;
  },
  deleteRun: async (runId) => {
    if (await change(() => teamBackend.deleteRun(runId))) {
      set({ deleteRunOpen: false });
    }
  },
  setTask: async (id, task, acceptance) =>
    (await change(() => teamBackend.setTask(id, task, acceptance))) !== null,
  addMember: async (input) => {
    const runId = get().runId;
    if (!runId) return false;
    const done = await change(() => teamBackend.addMember(runId, input));
    if (!done) return false;
    set((state) => ({
      selectedId: done.value.id,
      addOpen: false,
      // The parent shows its new member.
      expanded:
        input.kind === "agent" &&
        input.parentId &&
        !state.expanded.includes(input.parentId)
          ? [...state.expanded, input.parentId]
          : state.expanded,
    }));
    return true;
  },
}));

// Each change to the expansion or the runs is stored: every run on screen, with its expanded members.
useTeamStore.subscribe((state, previous) => {
  if (
    !state.loaded ||
    (state.expanded === previous.expanded && state.runs === previous.runs)
  ) {
    return;
  }
  const runOf = new Map(
    state.members.map((member) => [member.id, member.runId]),
  );
  storedExpanded = Object.fromEntries(
    state.runs.map((run) => [
      run.id,
      state.expanded.filter((id) => runOf.get(id) === run.id),
    ]),
  );
  writeStoredExpanded(storedExpanded);
});

/** Reads the team now and again each time the backend changes it. Returns the function that stops listening. */
export function watchTeam(): () => void {
  void useTeamStore.getState().load();
  return onTeamChanged(() => void useTeamStore.getState().load());
}
