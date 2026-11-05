import { create } from "zustand";

import {
  commands,
  type AgentStatus,
  type HarnessId,
  type TerminalInfo,
} from "@/bindings";
import { unwrap } from "@/lib/ipc-result";
import { useResumableStore } from "@/workspace/resumable-store";

// The real size is sent as soon as the view fits itself.
const INITIAL_COLS = 120;
const INITIAL_ROWS = 32;

interface TerminalState {
  sessions: TerminalInfo[];
  activeByWorktree: Record<string, string>;
  /** Asks a view to take keyboard focus, even when its tab was already the active one. */
  focusRequest: { id: string; nonce: number } | null;

  load: () => Promise<void>;
  create: (
    worktreePath: string,
    command: string | null,
  ) => Promise<TerminalInfo>;
  close: (id: string) => Promise<void>;
  /** The backend closed a session on its own: its tab goes as on a close, without asking the backend again. */
  dropClosed: (id: string) => void;
  closeAllIn: (worktreePath: string) => Promise<void>;
  setActive: (worktreePath: string, id: string) => void;
  requestFocus: (id: string) => void;
  markExited: (id: string, exitCode: number | null) => void;
  /** A session became an agent, stopped being one, or its agent changed status. */
  setAgentState: (
    id: string,
    harness: HarnessId | null,
    status: AgentStatus | null,
  ) => void;
  markSeen: (id: string) => void;
  /** A session was started by the backend, possibly for an agent: listed, not made active. */
  added: (session: TerminalInfo) => void;
  /** Freezes a terminal the user is not using. The backend refuses, with the reason, when it cannot. */
  hibernate: (id: string) => Promise<void>;
  /** The terminal got input, so the daemon has run it again. */
  woken: (id: string) => void;
}

/** Sessions the backend closed on its own. The team keeps their members, so a worker's card can show how it ended. */
const closedByBackend = new Set<string>();

/** Whether the backend closed this session on its own, rather than the person closing it. */
export function isClosedByBackend(id: string): boolean {
  return closedByBackend.has(id);
}

/** The list and the active tabs without one session. When it was the active tab, the tab next to it takes over. */
function withoutSession(
  state: Pick<TerminalState, "sessions" | "activeByWorktree">,
  id: string,
): Pick<TerminalState, "sessions" | "activeByWorktree"> {
  const worktreePath = state.sessions.find((s) => s.id === id)?.worktreePath;
  if (worktreePath === undefined) return state;
  const siblings = state.sessions.filter(
    (s) => s.worktreePath === worktreePath,
  );
  const index = siblings.findIndex((s) => s.id === id);
  const neighbor = siblings[index - 1] ?? siblings[index + 1];
  const activeByWorktree = { ...state.activeByWorktree };
  if (activeByWorktree[worktreePath] === id) {
    if (neighbor) activeByWorktree[worktreePath] = neighbor.id;
    else delete activeByWorktree[worktreePath];
  }
  return {
    sessions: state.sessions.filter((s) => s.id !== id),
    activeByWorktree,
  };
}

/** The list with the session at its end, unless it is already listed: the screen hears of its own
 * new sessions twice, from the event and from the command's answer. */
function withSession(sessions: TerminalInfo[], session: TerminalInfo) {
  return sessions.some((s) => s.id === session.id)
    ? sessions
    : [...sessions, session];
}

export const useTerminalStore = create<TerminalState>((set, get) => ({
  sessions: [],
  activeByWorktree: {},
  focusRequest: null,

  /** Sessions outlive a webview reload, so the list comes from the backend. */
  async load() {
    set({ sessions: await unwrap(commands.listTerminals()) });
  },

  async create(worktreePath, command) {
    const session = await unwrap(
      commands.createTerminal(
        worktreePath,
        command,
        INITIAL_COLS,
        INITIAL_ROWS,
      ),
    );
    set((state) => ({
      sessions: withSession(state.sessions, session),
      activeByWorktree: {
        ...state.activeByWorktree,
        [worktreePath]: session.id,
      },
    }));
    return session;
  },

  added(session) {
    set((state) => ({ sessions: withSession(state.sessions, session) }));
  },

  async close(id) {
    if (!get().sessions.some((s) => s.id === id)) return;

    // Forget it first so the tab disappears at once; the backend call only cleans up.
    set((state) => withoutSession(state, id));
    await commands.closeTerminal(id);
  },

  dropClosed(id) {
    if (!get().sessions.some((s) => s.id === id)) return;
    closedByBackend.add(id);
    set((state) => withoutSession(state, id));
  },

  async closeAllIn(worktreePath) {
    const ids = get()
      .sessions.filter((s) => s.worktreePath === worktreePath)
      .map((s) => s.id);
    await Promise.all(ids.map((id) => get().close(id)));
  },

  setActive(worktreePath, id) {
    set((state) => ({
      activeByWorktree: { ...state.activeByWorktree, [worktreePath]: id },
    }));
  },

  requestFocus(id) {
    set((state) => ({
      focusRequest: { id, nonce: (state.focusRequest?.nonce ?? 0) + 1 },
    }));
  },

  markExited(id, exitCode) {
    set((state) => ({
      sessions: state.sessions.map((s) =>
        s.id === id ? { ...s, running: false, exitCode, agentStatus: null } : s,
      ),
    }));
    // The backend records the end before it reports it, so the list read now includes it.
    const worktreePath = get().sessions.find((s) => s.id === id)?.worktreePath;
    if (worktreePath) void useResumableStore.getState().refresh(worktreePath);
  },

  setAgentState(id, harness, status) {
    set((state) => ({
      sessions: state.sessions.map((s) =>
        s.id === id ? { ...s, harness, agentStatus: status } : s,
      ),
    }));
  },

  async hibernate(id) {
    await unwrap(commands.hibernateTerminal(id));
    set((state) => ({
      sessions: state.sessions.map((s) =>
        s.id === id ? { ...s, hibernated: true } : s,
      ),
    }));
  },

  woken(id) {
    // Returns the same state when nothing changes, so typing does not redraw the tabs.
    set((state) =>
      state.sessions.some((s) => s.id === id && s.hibernated)
        ? {
            sessions: state.sessions.map((s) =>
              s.id === id ? { ...s, hibernated: false } : s,
            ),
          }
        : state,
    );
  },

  markSeen(id) {
    // The backend clears "done" and reports it back as a status change, so the store waits for that.
    void commands.markAgentSeen(id);
  },
}));

/** The tab that is showing for a worktree: the remembered one, else the newest. */
export function activeSessionId(
  sessions: readonly TerminalInfo[],
  activeByWorktree: Record<string, string>,
  worktreePath: string | null,
): string | null {
  if (!worktreePath) return null;
  const own = sessions.filter((s) => s.worktreePath === worktreePath);
  const remembered = activeByWorktree[worktreePath];
  if (remembered && own.some((s) => s.id === remembered)) return remembered;
  return own.at(-1)?.id ?? null;
}
