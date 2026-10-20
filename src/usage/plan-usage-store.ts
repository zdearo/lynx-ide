import { create } from "zustand";

import { commands, type PlanUsage } from "@/bindings";
import { unwrap } from "@/lib/ipc-result";

interface PlanUsageState {
  /** Null until the first read finishes. */
  entries: PlanUsage[] | null;
  /** Reads the plans again. `force` skips the backend's cache. Rejects only when the IPC call itself fails. */
  refresh: (force: boolean) => Promise<void>;
}

export const usePlanUsageStore = create<PlanUsageState>((set) => ({
  entries: null,
  async refresh(force) {
    set({ entries: await unwrap(commands.getPlanUsage(force)) });
  },
}));
