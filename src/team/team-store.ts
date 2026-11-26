import { toast } from "sonner";
import { create } from "zustand";
import { useShallow } from "zustand/react/shallow";

import type {
  BotSummary,
  HarnessId,
  NewTeamMember,
  TeamMember as TeamMemberRecord,
  TeamNote,
  TeamSnapshot,
  TerminalInfo,
} from "@/bindings";
import { TERMINAL_PRESETS } from "@/agents/agent-presets";
import { type BotProvider } from "@/bots/bot-colors";
import { isMockBackend } from "@/dev/mock-mode";
import { errorMessage } from "@/lib/error-message";
import {
  TEAM_DOCS_MEMBERS,
  TEAM_MEMBERS,
  TEAM_NEEDS_YOU_ID,
  type TeamMember,
  type TeamMemberBase,
  type TeamStatus,
  type WorkerState,
} from "@/team/team-fixtures";
import { teamBackend, onTeamChanged } from "@/team/team-backend";
import { isDrawn } from "@/team/team-hierarchy";
import type { SiblingOrder } from "@/team/team-layout";
import type { CardSize } from "@/team/team-canvas-model";
import { activityOf } from "@/team/team-live";
import type { TeamPair } from "@/team/team-pairs";
import { agentLine, requestCloseTerminal } from "@/terminal/terminal-actions";
import { isClosedByBackend, useTerminalStore } from "@/terminal/terminal-store";
import { useWorkspaceStore } from "@/workspace/workspace-store";
import { copy } from "@/ui-copy";

/** A worktree's central area: its terminals, or the team. */
export type TeamMode = "terminal" | "team";

/** Omit over each variant of a union, so the fields of one kind are not lost. */
type DistributiveOmit<T, K extends PropertyKey> = T extends unknown
  ? Omit<T, K>
  : never;

/** What the add dialog sends. The store fills in the worktree on the sidebar and, for an agent, its terminal. */
// A task is never set when a member is added: the orchestrator gives it later, and a member without one only talks.
export type MemberDraft = DistributiveOmit<
  NewTeamMember,
  "worktree" | "sessionId" | "task" | "acceptance" | "dependsOn"
>;

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
  /** Every worktree's members. The screen shows the ones in the worktree selected on the sidebar. */
  members: TeamMember[];
  pairs: TeamPair[];
  /** Every worktree's notes, as the backend keeps them. The screen shows the ones in the worktree selected on the sidebar. */
  notes: TeamNote[];
  /** The members whose own members are listed under them: each member's flag from the backend's snapshot. */
  expanded: string[];
  selectedId: string | null;
  /** How many rows each member sits below its default row, by id. */
  extraRows: Record<string, number>;
  /** The size of each card the user resized, by id. A card not listed has the default size. */
  cardSizes: Record<string, CardSize>;
  /** The left-to-right order of each member's own members, once the user has moved one sideways. */
  siblingOrder: SiblingOrder;
  modeByWorktree: Record<string, TeamMode>;
  /** Fits the drawn map on screen. Set while the map is on screen, null otherwise. */
  fitMap: (() => void) | null;
  /** The members whose terminal is open on the map, in the order they were opened. Kept per member, across reloads. */
  openIds: string[];
  /** Terminals still to open by a batch (open all, or the restore on load). The camera waits for them. */
  openingPending: number;
  /** The size of each open terminal the user resized, by id. A terminal not listed has the default size. */
  terminalSizes: Record<string, CardSize>;
  /** The members whose node is changing size, or being resized: their terminals hold the process's size until it lands. */
  movingIds: string[];
  /** Whether the add-member dialog is open. */
  addOpen: boolean;
  /** True while the add flow starts an agent's terminal. The team stays on screen then. */
  addingSession: boolean;

  /** Reads the team from the backend. A read that a newer one overtakes is dropped. */
  load: () => Promise<void>;
  setMode: (path: string, mode: TeamMode) => void;
  openTerminal: (id: string) => void;
  closeTerminal: (id: string) => void;
  openAll: () => void;
  closeAll: () => void;
  setTerminalSize: (id: string, size: CardSize) => void;
  setMoving: (id: string, moving: boolean) => void;
  select: (id: string | null) => void;
  /** Opens or closes a member's branch in the backend. Shows nothing until the team is read again. */
  toggle: (id: string) => Promise<void>;
  /** The changes below change the backend and show nothing until its event brings the team again. A refused one changes nothing. */
  dismiss: (id: string) => Promise<void>;
  setLinked: (from: string, to: string, linked: boolean) => Promise<void>;
  setOrchestrator: (id: string, on: boolean) => Promise<void>;
  /** Puts a member under an orchestrator, or under no one (null). */
  setParent: (id: string, parentId: string | null) => Promise<void>;
  setExtraRows: (id: string, rows: number) => void;
  setCardSize: (id: string, size: CardSize) => void;
  setSiblingOrder: (order: SiblingOrder) => void;
  setAddOpen: (open: boolean) => void;
  /** Adds a member to the worktree on the sidebar and picks it. False when it was refused, and the dialog keeps what was typed. */
  addMember: (input: MemberDraft) => Promise<boolean>;
  /**
   * Starts an agent member's session again in its worktree: its conversation when the list still has it, else a fresh
   * agent. The new session becomes the member's. False when it could not start or the team refused it.
   */
  reviveMember: (id: string) => Promise<boolean>;
}

/** A member the backend lists, with its live fields. One that lacks the agent or bot its kind names is refused, not guessed. */
function memberOf(record: TeamMemberRecord): TeamMember {
  const live = LIVE[record.id] ?? IDLE;
  const base = {
    id: record.id,
    worktree: record.worktree,
    role: record.role,
    parentId: record.parentId,
    orchestrator: record.orchestrator,
    task: record.task,
    acceptance: record.acceptance,
    taskState: record.taskState,
    dependsOn: record.dependsOn,
    executionWorktree: record.executionWorktree,
    workerState: workerStateOf(record.workerState),
    ...live,
    // The newest message is what the member last did. The mock's simulated activity stays for a member with none.
    activity: activityOf(record.lastMessage) ?? live.activity,
  };
  if (record.kind === "bot" && record.botId) {
    return { ...base, kind: "bot", botId: record.botId };
  }
  // An agent the orchestrator added has no harness until its first worker starts.
  if (record.kind === "agent") {
    return {
      ...base,
      kind: "agent",
      harness: record.harness,
      sessionId: record.sessionId,
    };
  }
  throw new Error(`team member ${record.id} is missing its bot`);
}

/** The status an agent's card shows, from its session: what the agent reports while it runs, "closed" once the session is gone or its program ended. */
export function statusOfSession(session: TerminalInfo | undefined): TeamStatus {
  if (!session?.running) return "closed";
  return session.agentStatus ?? "idle";
}

/** The worker state the backend sent, or null for a member that is no worker or a state Lynx does not know. */
function workerStateOf(value: string | null): WorkerState | null {
  return value === "running" ||
    value === "done" ||
    value === "failed" ||
    value === "stopped"
    ? value
    : null;
}

/** The status a worker's card shows: its worker state, where a stopped worker reads as closed. */
export function statusOfWorker(workerState: WorkerState): TeamStatus {
  switch (workerState) {
    case "running":
      return "working";
    case "done":
      return "done";
    case "failed":
      return "failed";
    case "stopped":
      return "closed";
  }
}

/** A member's card status from its worker state or its session, or null when it keeps the status it has (a bot, or an agent with no session). */
function statusOfMember(
  member: TeamMember,
  sessions: readonly TerminalInfo[],
): TeamStatus | null {
  if (member.workerState !== null) return statusOfWorker(member.workerState);
  if (member.kind !== "agent" || member.sessionId === null) return null;
  return statusOfSession(
    sessions.find((session) => session.id === member.sessionId),
  );
}

/** The members with each card's status taken from its worker or session. Returns the same array when nothing changed, so the screen does not redraw. */
function withSessionStatus(
  members: TeamMember[],
  sessions: readonly TerminalInfo[],
): TeamMember[] {
  let changed = false;
  const next = members.map((member) => {
    const status = statusOfMember(member, sessions);
    if (status === null || status === member.status) return member;
    changed = true;
    return { ...member, status };
  });
  return changed ? next : members;
}

/** Each member's object from the last read when its content is the same, and the last list itself when nothing changed. */
function keepUnchanged(
  members: TeamMember[],
  previous: TeamMember[],
): TeamMember[] {
  const before = new Map(previous.map((member) => [member.id, member]));
  const kept = members.map((member) => {
    const old = before.get(member.id);
    return old && JSON.stringify(old) === JSON.stringify(member) ? old : member;
  });
  const unchanged =
    kept.length === previous.length &&
    kept.every((member, index) => member === previous[index]);
  return unchanged ? previous : kept;
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

/** Opens a member's branch in the backend when it is shut, so the members under it show. */
async function openBranch(id: string): Promise<void> {
  if (useTeamStore.getState().expanded.includes(id)) return;
  await change(() => teamBackend.setExpanded(id, true));
}

/** Members the user dismissed. Their sessions go by the user's close choice, so a team read that drops them closes nothing. */
const dismissedMemberIds = new Set<string>();

/** The mock's seeded members. They have no session and show the simulated output their fixtures carry. */
const SEEDED_IDS: ReadonlySet<string> = new Set(
  [...TEAM_MEMBERS, ...TEAM_DOCS_MEMBERS].map((member) => member.id),
);

/** Whether a member is one of the mock's seeded fixtures. */
export function isSeededMember(id: string): boolean {
  return SEEDED_IDS.has(id);
}

/**
 * Starts a fresh agent's terminal in a worktree, as the terminal menu starts it. Null when it could not start; the
 * reason is shown.
 */
async function startAgentSession(
  worktree: string,
  harness: HarnessId,
): Promise<TerminalInfo | null> {
  try {
    const command = await agentLine(harness);
    if (command === null) throw new Error(`no terminal preset for ${harness}`);
    return await useTerminalStore.getState().create(worktree, command);
  } catch (error) {
    toast.error(copy.terminal.createFailed, {
      description: errorMessage(error),
    });
    return null;
  }
}

/**
 * Starts a session the team is about to give a member. Nothing is adopted while it starts. The session is taken in the
 * same step the flag clears, so no terminal change can adopt it before its member is saved.
 */
async function startMemberSession(
  worktree: string,
  harness: HarnessId,
): Promise<TerminalInfo | null> {
  useTeamStore.setState({ addingSession: true });
  try {
    const session = await startAgentSession(worktree, harness);
    if (session) takenSessionIds.add(session.id);
    return session;
  } finally {
    useTeamStore.setState({ addingSession: false });
  }
}

/** Closes a session without asking. A failure is ignored: the screen has already decided what to show. */
async function closeSessionQuietly(id: string): Promise<void> {
  try {
    await useTerminalStore.getState().close(id);
  } catch {
    // The backend refused to close it; there is nothing more the screen can do.
  }
}

/** Closes the sessions of agents that left the team, whatever took them out of it. */
function closeVanishedSessions(
  before: readonly TeamMember[],
  after: readonly TeamMember[],
): void {
  const stillInTeam = new Set(after.map((member) => member.id));
  for (const member of before) {
    if (stillInTeam.has(member.id)) continue;
    // Gone now: a dismissed id is forgotten, so a new member that takes the same id is watched again.
    const dismissed = dismissedMemberIds.delete(member.id);
    if (member.kind !== "agent" || !member.sessionId || dismissed) continue;
    void closeSessionQuietly(member.sessionId);
  }
}

/**
 * Sessions the team has taken: its own adds and the adoptions it started. Kept for the app run, so a session is taken
 * once even before a team read brings its member back, and a member dismissed while its terminal stays open is not
 * taken again.
 */
const takenSessionIds = new Set<string>();

/**
 * Adds each terminal agent that no member has as a loose member of its own worktree's team, as "No one" does. A
 * session the team's own add is starting is left to that add. Nothing is added before the team has been read once,
 * since a member the team already has would be taken twice; the next read or terminal change tries again.
 */
function adoptLooseAgents(sessions: readonly TerminalInfo[]): void {
  const { loaded, members, addingSession } = useTeamStore.getState();
  for (const session of sessions) {
    if (!session.harness || takenSessionIds.has(session.id)) continue;
    if (
      members.some(
        (member) => member.kind === "agent" && member.sessionId === session.id,
      )
    ) {
      takenSessionIds.add(session.id);
      continue;
    }
    if (!loaded || addingSession) continue;
    takenSessionIds.add(session.id);
    void adoptSession(session, session.harness);
  }
}

/** Adds a terminal agent to the team of the worktree it runs in. Silent when the team refuses it: the terminal tab already shows the session. */
async function adoptSession(
  session: TerminalInfo,
  harness: HarnessId,
): Promise<void> {
  try {
    const member = await teamBackend.addMember({
      kind: "agent",
      role: agentNameOf(harness),
      harness,
      parentId: null,
      orchestrator: false,
      dependsOn: [],
      task: "",
      acceptance: "",
      worktree: session.worktreePath,
      sessionId: session.id,
    });
    // It closed while the team added it, so its close was not seen and its card would stay: it goes now.
    if (
      !useTerminalStore.getState().sessions.some((s) => s.id === session.id)
    ) {
      void dismissClosedAgent(member.id);
    }
  } catch {
    // Silent on purpose: see above.
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
  if (member.kind === "agent") {
    return member.harness ? agentNameOf(member.harness) : copy.team.noAgentYet;
  }
  const bot = botOf(member, bots);
  if (!bot) return "";
  const provider =
    copy.bots.providers[bot.provider as BotProvider] ?? bot.provider;
  return copy.team.engine(provider, bot.model);
}

// The open terminals are kept per member in this viewer's browser: a convenience, so the map comes back as it was.
const OPEN_KEY = "lynx.team.open";

function readOpenIds(): string[] {
  try {
    const parsed: unknown = JSON.parse(localStorage.getItem(OPEN_KEY) ?? "[]");
    return Array.isArray(parsed)
      ? parsed.filter((id) => typeof id === "string")
      : [];
  } catch {
    return [];
  }
}

function saveOpenIds(openIds: readonly string[]) {
  try {
    localStorage.setItem(OPEN_KEY, JSON.stringify(openIds));
  } catch {
    // Not remembering is fine.
  }
}

/** Opens a member's terminal, or a bot's chat, on its card; the member is picked too, as the card's double click does. */
export function openMember(id: string) {
  const store = useTeamStore.getState();
  store.select(id);
  store.openTerminal(id);
}

/** The members of one worktree's team. None when no worktree is given. */
export function membersOfWorktree(
  members: readonly TeamMember[],
  worktree: string | null,
): TeamMember[] {
  return worktree === null
    ? []
    : members.filter((member) => member.worktree === worktree);
}

/** The members of one worktree's team. The same array while that team does not change, so its canvas keeps its layout. */
export function useTeamMembersOf(worktree: string | null): TeamMember[] {
  return useTeamStore(
    useShallow((state) => membersOfWorktree(state.members, worktree)),
  );
}

/** The team on screen: the members of the worktree selected on the sidebar. */
export function useTeamMembers(): TeamMember[] {
  const worktree = useWorkspaceStore((state) => state.selectedWorktreePath);
  return useTeamMembersOf(worktree);
}

/** A member's role by id, or undefined once it is gone from the team. */
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

/** The worktrees whose central area is the team. Each keeps its team mounted, hidden, while the sidebar is elsewhere. */
export function teamWorktreesOf(
  modeByWorktree: Record<string, TeamMode>,
): string[] {
  return Object.keys(modeByWorktree).filter(
    (path) => modeByWorktree[path] === "team",
  );
}

/**
 * The team the backend has sent. The top orchestrator of each tree opens the first time it is seen, so its members
 * show. After that the user's choice is stored, and a reload keeps it.
 */
function teamOf(
  snapshot: TeamSnapshot,
  selectedId: string | null,
  previous: TeamMember[],
): Partial<TeamState> {
  return {
    expanded: snapshot.members
      .filter((member) => member.expanded)
      .map((member) => member.id),
    loaded: true,
    members: keepUnchanged(
      withSessionStatus(
        snapshot.members.map(memberOf),
        useTerminalStore.getState().sessions,
      ),
      previous,
    ),
    pairs: snapshot.pairs.map(({ a, b }) => ({ from: a, to: b })),
    notes: snapshot.notes,
    // A picked member that the team no longer has is not picked.
    selectedId: snapshot.members.some((member) => member.id === selectedId)
      ? selectedId
      : null,
  };
}

/** The newest read of the team. Only it may write, so a slow earlier read cannot overwrite it. */
let latestLoad = 0;

/** Members whose agent is being started again. A second request for one of them does nothing. */
const revivingIds = new Set<string>();

/** How many terminals open per frame when they are opened all at once: each one mounts a terminal, and a frame holds one. */
export const OPEN_PER_FRAME = 1;

// The terminals still to open by "open all", and the frame that opens the next batch.
let opening: string[] = [];
let openingFrame: number | null = null;
// The terminals open when the app last closed: they reopen a few per frame once the team is on screen.
let restoring: string[] = readOpenIds();

function cancelOpening() {
  if (openingFrame !== null) cancelAnimationFrame(openingFrame);
  openingFrame = null;
  opening = [];
  useTeamStore.setState({ openingPending: 0 });
}

function openNextBatch() {
  openingFrame = null;
  const batch = opening.splice(0, OPEN_PER_FRAME);
  useTeamStore.setState((state) => {
    const fresh = batch.filter(
      (id) =>
        !state.openIds.includes(id) &&
        state.members.some((member) => member.id === id),
    );
    return {
      openingPending: opening.length,
      ...(fresh.length > 0 ? { openIds: [...state.openIds, ...fresh] } : {}),
    };
  });
  if (opening.length > 0) openingFrame = requestAnimationFrame(openNextBatch);
}

/** Reopens the terminals of the last session, a few per frame, once the team is on the screen. */
export function restoreOpenTerminals() {
  if (restoring.length === 0) return;
  cancelOpening();
  opening = restoring;
  restoring = [];
  useTeamStore.setState({ openingPending: opening.length });
  openingFrame = requestAnimationFrame(openNextBatch);
}

/** The team on screen. Its members and lines come from the backend; the rest is the screen's own state. */
export const useTeamStore = create<TeamState>((set, get) => ({
  loaded: false,
  members: [],
  pairs: [],
  notes: [],
  expanded: [],
  // The mock opens on its seeded pick; the real app starts with nothing picked.
  selectedId: isMockBackend() ? TEAM_NEEDS_YOU_ID : null,
  extraRows: {},
  cardSizes: {},
  siblingOrder: {},
  modeByWorktree: {},
  fitMap: null,
  openIds: [],
  openingPending: 0,
  terminalSizes: {},
  movingIds: [],
  addOpen: false,
  addingSession: false,

  load: async () => {
    const sequence = ++latestLoad;
    try {
      const snapshot = await teamBackend.snapshot();
      if (sequence !== latestLoad) return;
      const { loaded, members: before } = get();
      set((state) => teamOf(snapshot, state.selectedId, state.members));
      // A terminal open on a member who left the team has nothing to show.
      const { openIds, members } = get();
      const kept = openIds.filter((id) =>
        members.some((member) => member.id === id),
      );
      if (kept.length !== openIds.length) set({ openIds: kept });
      // The first read has no earlier team, so no member can have left it.
      if (loaded) closeVanishedSessions(before, get().members);
      // A read makes the team ready for the sessions that waited for it.
      adoptLooseAgents(useTerminalStore.getState().sessions);
    } catch (error) {
      toast.error(copy.team.loadFailed, { description: errorMessage(error) });
    }
  },
  setMode: (path, mode) =>
    set((state) => ({
      modeByWorktree: { ...state.modeByWorktree, [path]: mode },
    })),
  openTerminal: (id) => {
    set((state) => ({
      openIds: [...state.openIds.filter((open) => open !== id), id],
    }));
  },
  closeTerminal: (id) => {
    opening = opening.filter((pending) => pending !== id);
    set((state) => ({
      openIds: state.openIds.filter((open) => open !== id),
    }));
  },
  // Every terminal of the worktree on the screen, and none of it: the others keep their own state.
  // A few terminals open per frame: opening them all in one frame would mount them all in one frame.
  openAll: () => {
    cancelOpening();
    opening = membersOfWorktree(
      get().members,
      useWorkspaceStore.getState().selectedWorktreePath,
    ).map((member) => member.id);
    set({ openingPending: opening.length });
    // The first batch waits for the next frame: opened inside the click, it would share the click's frame.
    openingFrame = requestAnimationFrame(openNextBatch);
  },
  closeAll: () => {
    cancelOpening();
    set((state) => {
      const shown = new Set(
        membersOfWorktree(
          state.members,
          useWorkspaceStore.getState().selectedWorktreePath,
        ).map((member) => member.id),
      );
      return { openIds: state.openIds.filter((id) => !shown.has(id)) };
    });
  },
  setTerminalSize: (id, size) =>
    set((state) => ({
      terminalSizes: { ...state.terminalSizes, [id]: size },
    })),
  setMoving: (id, moving) =>
    set((state) => ({
      movingIds: moving
        ? [...state.movingIds.filter((moved) => moved !== id), id]
        : state.movingIds.filter((moved) => moved !== id),
    })),
  select: (selectedId) => set({ selectedId }),
  toggle: async (id) => {
    const open = get().expanded.includes(id);
    const done = await change(() => teamBackend.setExpanded(id, !open));
    // A collapse that hides the picked member clears the pick, as a dismiss does. The next team read shows the rest.
    if (done && open) {
      const { selectedId, members, expanded } = get();
      if (
        selectedId !== null &&
        !isDrawn(
          members,
          expanded.filter((other) => other !== id),
          selectedId,
        )
      ) {
        set({ selectedId: null });
      }
    }
  },
  setOrchestrator: async (id, on) => {
    await change(() => teamBackend.setOrchestrator(id, on));
  },
  setParent: async (id, parentId) => {
    if (!(await change(() => teamBackend.setParent(id, parentId)))) return;
    // This member goes last on its new orchestrator's row, at its default level.
    set((state) => ({
      extraRows: { ...state.extraRows, [id]: 0 },
      siblingOrder: Object.fromEntries(
        Object.entries(state.siblingOrder).map(([key, ids]) => [
          key,
          ids.filter((other) => other !== id),
        ]),
      ),
    }));
    if (parentId) await openBranch(parentId);
  },
  dismiss: async (id) => {
    // Marked before the call, so a team read that the backend sends at once cannot close the session first.
    dismissedMemberIds.add(id);
    const done = await change(() => teamBackend.dismissMember(id));
    if (!done) {
      dismissedMemberIds.delete(id);
      return;
    }
    set((state) => ({
      selectedId: state.selectedId === id ? null : state.selectedId,
    }));
    // An agent's terminal follows the close policy: a running session asks first. A bot has none.
    if (done.value) requestCloseTerminal(done.value);
  },
  setLinked: async (from, to, linked) => {
    await change(() => teamBackend.setLinked(from, to, linked));
  },
  setExtraRows: (id, rows) =>
    set((state) => ({
      extraRows: { ...state.extraRows, [id]: Math.max(0, rows) },
    })),
  setCardSize: (id, size) =>
    set((state) => ({ cardSizes: { ...state.cardSizes, [id]: size } })),
  setSiblingOrder: (siblingOrder) => set({ siblingOrder }),
  setAddOpen: (addOpen) => set({ addOpen }),
  addMember: async (draft) => {
    const worktree = useWorkspaceStore.getState().selectedWorktreePath;
    if (!worktree) {
      toast.error(copy.team.add.noWorktree);
      return false;
    }
    // Picks the new member, and opens its parent so the member shows.
    const showAdded = async (
      added: TeamMemberRecord,
      parentId: string | null,
    ) => {
      set({ selectedId: added.id, addOpen: false });
      if (parentId) await openBranch(parentId);
      return true;
    };

    if (draft.kind === "bot") {
      // A bot runs no session: it joins this worktree's team.
      const done = await change(() =>
        teamBackend.addMember({ ...draft, worktree }),
      );
      return done ? showAdded(done.value, null) : false;
    }

    // The agent starts right away in this worktree, and the member keeps that session.
    const harness = draft.harness;
    if (!harness) return false;
    const session = await startMemberSession(worktree, harness);
    if (!session) return false;

    const done = await change(() =>
      teamBackend.addMember({
        ...draft,
        harness,
        dependsOn: [],
        task: "",
        acceptance: "",
        worktree,
        sessionId: session.id,
      }),
    );
    if (!done) {
      // The team refused the member, so the session started for it is closed again.
      await closeSessionQuietly(session.id);
      return false;
    }
    return showAdded(done.value, draft.parentId);
  },
  reviveMember: async (id) => {
    const member = get().members.find((candidate) => candidate.id === id);
    if (member?.kind !== "agent" || !member.harness || revivingIds.has(id)) {
      return false;
    }
    if (revivingIds.has(id)) return false;
    // A session still open is not started again: the member would have two terminals.
    if (
      useTerminalStore
        .getState()
        .sessions.some((session) => session.id === member.sessionId)
    ) {
      return false;
    }
    // Set before the first await, so a second click while this one runs starts nothing.
    revivingIds.add(id);
    try {
      const session = await startMemberSession(member.worktree, member.harness);
      if (!session) return false;

      const tied = await change(() => teamBackend.setSession(id, session.id));
      if (!tied) {
        // The member keeps its old session, so the one just started belongs to no one: it closes.
        await closeSessionQuietly(session.id);
        return false;
      }
      await get().load();
      return true;
    } finally {
      revivingIds.delete(id);
    }
  },
}));

// The pick and an open terminal belong to the team on screen: a worktree switch leaves neither behind.
useWorkspaceStore.subscribe((state, previous) => {
  if (state.selectedWorktreePath !== previous.selectedWorktreePath) {
    useTeamStore.setState({ selectedId: null });
  }
});

/** Reads the team now and again each time the backend changes it. Returns the function that stops listening. */
export function watchTeam(): () => void {
  void useTeamStore.getState().load();
  return onTeamChanged(() => void useTeamStore.getState().load());
}

/**
 * Dismisses an agent whose terminal was closed, from the terminal tabs or from the team: the terminal is the
 * agent's life here, so its card goes with it. Silent when the team no longer has it (it was dismissed first).
 */
async function dismissClosedAgent(id: string): Promise<void> {
  try {
    await teamBackend.dismissMember(id);
  } catch {
    // Already gone from the team; nothing more to do.
  }
}

/** Takes each agent's status from its session, after the terminal store changed. */
function syncSessionStatus(sessions: readonly TerminalInfo[]): void {
  const { members } = useTeamStore.getState();
  const next = withSessionStatus(members, sessions);
  if (next !== members) useTeamStore.setState({ members: next });
}

// A terminal change can start, end or close an agent: its card follows, a new agent joins its worktree's team, and a
// session the store drops is closed with its agent, which leaves the team.
useTerminalStore.subscribe((state, previous) => {
  if (state.sessions === previous.sessions) return;
  syncSessionStatus(state.sessions);
  adoptLooseAgents(state.sessions);
  // A worker's card keeps the state the backend gave it, so when a worker's process starts or ends, read the team again.
  const running = (sessions: readonly TerminalInfo[], id: string) =>
    sessions.find((session) => session.id === id)?.running ?? false;
  const workerChanged = useTeamStore
    .getState()
    .members.some(
      (member) =>
        member.kind === "agent" &&
        member.workerState !== null &&
        member.sessionId !== null &&
        running(state.sessions, member.sessionId) !==
          running(previous.sessions, member.sessionId),
    );
  if (workerChanged) void useTeamStore.getState().load();
  const kept = new Set(state.sessions.map((session) => session.id));
  const closed = new Set(
    previous.sessions
      .filter((session) => !kept.has(session.id))
      .map((session) => session.id),
  );
  if (closed.size === 0) return;
  for (const member of useTeamStore.getState().members) {
    if (member.kind !== "agent" || !member.sessionId) continue;
    if (!closed.has(member.sessionId) || dismissedMemberIds.has(member.id)) {
      continue;
    }
    // A worker the backend closed keeps its card, which shows how it ended.
    if (isClosedByBackend(member.sessionId)) continue;
    dismissedMemberIds.add(member.id);
    void dismissClosedAgent(member.id);
  }
});

// The open terminals are kept as they change, so a reload brings them back.
// Saved once the terminals are all open: a partial list written in between would lose the ones still to come.
useTeamStore.subscribe((state, previous) => {
  if (
    state.openIds !== previous.openIds &&
    restoring.length === 0 &&
    opening.length === 0
  ) {
    saveOpenIds(state.openIds);
  }
});
