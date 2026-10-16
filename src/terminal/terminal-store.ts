import { create } from "zustand";

import { commands, type TerminalInfo } from "@/bindings";
import { unwrap } from "@/lib/ipc-result";

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
  closeAllIn: (worktreePath: string) => Promise<void>;
  setActive: (worktreePath: string, id: string) => void;
  requestFocus: (id: string) => void;
  markExited: (id: string, exitCode: number | null) => void;
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
      sessions: [...state.sessions, session],
      activeByWorktree: {
        ...state.activeByWorktree,
        [worktreePath]: session.id,
      },
    }));
    return session;
  },

  async close(id) {
    const session = get().sessions.find((s) => s.id === id);
    if (!session) return;

    // Forget it first so the tab disappears at once; the backend call only cleans up.
    set((state) => {
      const siblings = state.sessions.filter(
        (s) => s.worktreePath === session.worktreePath,
      );
      const index = siblings.findIndex((s) => s.id === id);
      const neighbor = siblings[index - 1] ?? siblings[index + 1];
      const activeByWorktree = { ...state.activeByWorktree };
      if (activeByWorktree[session.worktreePath] === id) {
        if (neighbor) activeByWorktree[session.worktreePath] = neighbor.id;
        else delete activeByWorktree[session.worktreePath];
      }
      return {
        sessions: state.sessions.filter((s) => s.id !== id),
        activeByWorktree,
      };
    });
    await commands.closeTerminal(id);
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
        s.id === id ? { ...s, running: false, exitCode } : s,
      ),
    }));
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
