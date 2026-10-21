import { create } from "zustand";

import {
  commands,
  type BrowserPageInfo,
  type BrowserState,
  type EngineProgress,
  type EngineStatus,
  type IpcErrorKind,
} from "@/bindings";
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
  page: null,
};

/** An agent in control of the shown page: who it is and what it is doing. */
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
  /** What is installed, or null until the first read. */
  engine: EngineStatus | null;
  /** How far an install has got, while one runs. */
  engineProgress: EngineProgress | null;
  /** An install or a removal is running. */
  engineBusy: boolean;
  /** Why the last install or removal failed. */
  engineError: string | null;
  /** What an agent is doing in the browser right now, or null a few seconds after it stops. */
  agentAction: AgentAction | null;
  /** Every page, for every owner, as the backend last reported them. */
  pages: BrowserPageInfo[];
  /** The last picture of each page, as base64 PNG without a data URL prefix. */
  thumbnails: Record<number, string>;

  setOpen: (open: boolean) => void;
  toggle: () => void;
  /** Starts the engine if it is not running, and opens `url` when given. */
  start: (url?: string) => Promise<void>;
  /** Goes to what was typed in the address bar. */
  navigate: (input: string) => Promise<void>;
  /** Moves a tab to a window of its own. */
  detachPage: (page: number) => Promise<void>;
  /** Brings a tab in its own window back to the main window. */
  attachPage: (page: number) => Promise<void>;
  /** Makes a tab the one the agents of its worktree use. */
  assignAgentPage: (page: number) => Promise<void>;
  apply: (state: BrowserState) => void;
  loadEngine: () => Promise<void>;
  /** Downloads the engine. With `force`, an engine of the same version is installed again. */
  installEngine: (force?: boolean) => Promise<void>;
  removeEngine: () => Promise<void>;
  applyEngineProgress: (progress: EngineProgress) => void;
  /** An agent used a page: remembered only when that page is the one shown. */
  noteAgentAction: (page: number, agent: string, summary: string) => void;
  /** No agent is in control any more. */
  releaseAgent: () => void;
  /** Reads the list of pages and their pictures. */
  loadPages: () => Promise<void>;
  applyPages: (pages: BrowserPageInfo[]) => void;
  applyThumbnail: (page: number, png: string) => void;
  /** Opens a new tab for a worktree, shows it, and goes to `url` when given. */
  newPage: (worktreePath: string, url?: string | null) => Promise<void>;
  /** Shows a tab to the person. */
  showPage: (page: number) => Promise<void>;
  /** Closes a tab. Nothing is shown if it was the shown one. */
  closePage: (page: number) => Promise<void>;
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
  engine: null,
  engineProgress: null,
  engineBusy: false,
  engineError: null,
  agentAction: null,
  pages: [],
  thumbnails: {},

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

  detachPage: async (page) => {
    try {
      await unwrap(commands.browserPageDetach(page));
      set({ failure: null });
    } catch (error) {
      set({ failure: failureOf(error) });
    }
  },

  attachPage: async (page) => {
    try {
      await unwrap(commands.browserPageAttach(page));
      set({ failure: null });
    } catch (error) {
      set({ failure: failureOf(error) });
    }
  },

  assignAgentPage: async (page) => {
    try {
      await unwrap(commands.browserPageAssignAgent(page));
      set({ failure: null });
    } catch (error) {
      set({ failure: failureOf(error) });
    }
  },

  apply: (state) => set({ state }),

  loadEngine: async () => {
    try {
      set({ engine: await unwrap(commands.browserEngineStatus()) });
    } catch (error) {
      set({ engineError: errorMessage(error) });
    }
  },

  installEngine: async (force = false) => {
    if (get().engineBusy) return;
    set({ engineBusy: true, engineError: null, engineProgress: null });
    try {
      const engine = await unwrap(commands.browserEngineInstall(force));
      set({ engine, failure: null });
      // The person was waiting to use the browser: start it now that it can run.
      if (get().open && get().state.status !== "running") void get().start();
    } catch (error) {
      set({ engineError: errorMessage(error) });
    } finally {
      set({ engineBusy: false, engineProgress: null });
    }
  },

  removeEngine: async () => {
    if (get().engineBusy) return;
    set({ engineBusy: true, engineError: null });
    try {
      set({ engine: await unwrap(commands.browserEngineRemove()) });
    } catch (error) {
      set({ engineError: errorMessage(error) });
    } finally {
      set({ engineBusy: false });
    }
  },

  applyEngineProgress: (engineProgress) => set({ engineProgress }),

  noteAgentAction: (page, agent, summary) => {
    // An agent works its own tab. Only the tab the person sees gets the mark in the toolbar.
    if (page !== get().state.page) return;
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

  loadPages: async () => {
    try {
      const [pages, thumbnails] = await Promise.all([
        unwrap(commands.browserPages()),
        unwrap(commands.browserThumbnails()),
      ]);
      set({
        pages,
        thumbnails: Object.fromEntries(
          thumbnails.map((thumbnail) => [thumbnail.page, thumbnail.png]),
        ),
      });
    } catch (error) {
      set({ failure: failureOf(error) });
    }
  },

  applyPages: (pages) => set({ pages }),

  applyThumbnail: (page, png) =>
    set((state) => ({ thumbnails: { ...state.thumbnails, [page]: png } })),

  newPage: async (worktreePath, url = null) => {
    set({ open: true });
    try {
      await unwrap(commands.browserPageNew(worktreePath, url));
      set({ failure: null });
    } catch (error) {
      set({ failure: failureOf(error) });
    }
  },

  showPage: async (page) => {
    set({ open: true });
    try {
      const state = await unwrap(commands.browserPageShow(page));
      set({ state, failure: null });
    } catch (error) {
      set({ failure: failureOf(error) });
    }
  },

  closePage: async (page) => {
    try {
      await unwrap(commands.browserPageClose(page));
      set({ failure: null });
    } catch (error) {
      set({ failure: failureOf(error) });
    }
  },
}));
