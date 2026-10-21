import { create } from "zustand";

import { commands, type UpdateStatus } from "@/bindings";
import { unwrap } from "@/lib/ipc-result";

interface UpdateState {
  /** Null until the backend sends the first status. */
  status: UpdateStatus | null;
  load: () => Promise<void>;
  /** Takes a status the backend sent on its own, when a check or a download moved. */
  apply: (status: UpdateStatus) => void;
  check: () => Promise<void>;
  install: () => Promise<void>;
  restart: () => Promise<void>;
}

/** The update status as the backend reports it. Every action goes through the backend. */
export const useUpdateStore = create<UpdateState>((set) => ({
  status: null,

  async load() {
    set({ status: await unwrap(commands.getUpdateStatus()) });
  },

  apply(status) {
    set({ status });
  },

  async check() {
    set({ status: await unwrap(commands.checkForUpdates()) });
  },

  async install() {
    set({ status: await unwrap(commands.installUpdate()) });
  },

  async restart() {
    await commands.restartToUpdate();
  },
}));
