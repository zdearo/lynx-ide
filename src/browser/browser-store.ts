import { create } from "zustand";

import { commands, type BrowserState, type IpcErrorKind } from "@/bindings";
import { errorMessage } from "@/lib/error-message";
import { IpcRequestError, unwrap } from "@/lib/ipc-result";

/** Why the app could not do what was asked: the engine is missing, the address is not a web page. */
export interface BrowserFailure {
  kind: IpcErrorKind | null;
  message: string;
}

export const STOPPED: BrowserState = {
  status: "stopped",
  url: "",
  title: "",
  loading: false,
  canGoBack: false,
  canGoForward: false,
  error: null,
  detached: false,
};

/** An agent in control of the page: who it is and what it is doing. */
export interface AgentAction {
  agent: string;
  summary: string;
}

/** The backend says when control ends; this only clears the mark if that message never comes. */
const AGENT_MARK_MS = 40_000;
let agentMarkTimer: ReturnType<typeof setTimeout> | undefined;

interface BrowserStoreState {
  /** The browser is shown in place of the terminals. */
  open: boolean;
  /** What the engine reports; the backend keeps it and tells us when it changes. */
  state: BrowserState;
  /** A request to start the engine is in flight. */
  starting: boolean;
  failure: BrowserFailure | null;
  /** What an agent is doing in the browser right now, or null a few seconds after it stops. */
  agentAction: AgentAction | null;

  setOpen: (open: boolean) => void;
  toggle: () => void;
  /** Starts the engine if it is not running, and opens `url` when given. */
  start: (url?: string) => Promise<void>;
  /** Goes to what was typed in the address bar. */
  navigate: (input: string) => Promise<void>;
  /** Moves the page to a window of its own. */
  detach: () => Promise<void>;
  /** Brings the page back to the main window. */
  attach: () => Promise<void>;
  apply: (state: BrowserState) => void;
  /** An agent used the browser: remember who and what, for whoever has the browser open. */
  noteAgentAction: (agent: string, summary: string) => void;
  /** No agent is in control any more. */
  releaseAgent: () => void;
}

function failureOf(error: unknown): BrowserFailure {
  return {
    kind: error instanceof IpcRequestError ? error.kind : null,
    message: errorMessage(error),
  };
}

export const useBrowserStore = create<BrowserStoreState>((set, get) => ({
  open: false,
  state: STOPPED,
  starting: false,
  failure: null,
  agentAction: null,

  setOpen: (open) => {
    set({ open });
    const { state, starting } = get();
    // Opening the view is what starts the engine; it is not started with the app.
    if (open && !starting && state.status !== "running") void get().start();
  },

  toggle: () => get().setOpen(!get().open),

  start: async (url) => {
    if (get().starting) return;
    set({ starting: true, failure: null });
    try {
      const state = await unwrap(commands.browserOpen(url ?? null));
      set({ state });
    } catch (error) {
      set({ failure: failureOf(error) });
    } finally {
      set({ starting: false });
    }
  },

  navigate: async (input) => {
    if (get().state.status !== "running") return get().start(input);
    try {
      await unwrap(commands.browserNavigate(input));
      set({ failure: null });
    } catch (error) {
      set({ failure: failureOf(error) });
    }
  },

  detach: async () => {
    try {
      await unwrap(commands.browserDetach());
      set({ failure: null });
    } catch (error) {
      set({ failure: failureOf(error) });
    }
  },

  attach: async () => {
    try {
      await unwrap(commands.browserAttach());
    } catch (error) {
      set({ failure: failureOf(error) });
    }
  },

  apply: (state) => set({ state }),

  noteAgentAction: (agent, summary) => {
    // The browser is not opened for the person: an agent works the page in the background, and
    // whoever wants to watch opens it.
    set({ agentAction: { agent, summary } });
    clearTimeout(agentMarkTimer);
    agentMarkTimer = setTimeout(
      () => set({ agentAction: null }),
      AGENT_MARK_MS,
    );
  },

  releaseAgent: () => {
    clearTimeout(agentMarkTimer);
    set({ agentAction: null });
  },
}));
