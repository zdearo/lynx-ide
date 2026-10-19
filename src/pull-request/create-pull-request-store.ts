import { create } from "zustand";

import { commands, type IpcErrorKind, type PullRequestForm } from "@/bindings";
import { errorMessage } from "@/lib/error-message";
import { IpcRequestError, unwrap } from "@/lib/ipc-result";

/** What the user typed. A `null` base or title is untouched, so the suggestion shows through. */
export interface PullRequestDraftFields {
  base: string | null;
  title: string | null;
  body: string;
  draft: boolean;
}

export type FormInfo =
  | { status: "loading" }
  | { status: "ready"; form: PullRequestForm }
  | { status: "error"; kind: IpcErrorKind; message: string };

const EMPTY: PullRequestDraftFields = {
  base: null,
  title: null,
  body: "",
  draft: false,
};

interface CreatePullRequestState {
  infos: Record<string, FormInfo>;
  drafts: Record<string, PullRequestDraftFields>;
  /** Asks the backend for the bases, the default and a suggested title. */
  loadInfo: (worktreePath: string) => Promise<void>;
  update: (
    worktreePath: string,
    change: Partial<PullRequestDraftFields>,
  ) => void;
  clear: (worktreePath: string) => void;
}

export const useCreatePullRequestStore = create<CreatePullRequestState>(
  (set) => ({
    infos: {},
    drafts: {},

    async loadInfo(worktreePath) {
      let info: FormInfo;
      try {
        const form = await unwrap(commands.getPullRequestForm(worktreePath));
        info = { status: "ready", form };
      } catch (error) {
        info =
          error instanceof IpcRequestError
            ? { status: "error", kind: error.kind, message: error.message }
            : {
                status: "error",
                kind: "internal",
                message: errorMessage(error),
              };
      }
      set((state) => ({ infos: { ...state.infos, [worktreePath]: info } }));
    },

    update(worktreePath, change) {
      set((state) => ({
        drafts: {
          ...state.drafts,
          [worktreePath]: {
            ...(state.drafts[worktreePath] ?? EMPTY),
            ...change,
          },
        },
      }));
    },

    clear(worktreePath) {
      set((state) => ({ drafts: { ...state.drafts, [worktreePath]: EMPTY } }));
    },
  }),
);

export function draftFieldsOf(
  drafts: Record<string, PullRequestDraftFields>,
  worktreePath: string,
): PullRequestDraftFields {
  return drafts[worktreePath] ?? EMPTY;
}
