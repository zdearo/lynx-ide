import { create } from "zustand";

import {
  commands,
  type AppConfig,
  type ConfigSnapshot,
  type TerminalConfig,
} from "@/bindings";
import { unwrap } from "@/lib/ipc-result";

/** Mirrors the backend defaults, for the moment before the first config arrives. */
export const DEFAULT_TERMINAL_CONFIG: TerminalConfig = {
  fontFamily: null,
  fontSize: 13,
  shell: null,
  scrollback: 5000,
};

interface ConfigState {
  snapshot: ConfigSnapshot | null;
  load: () => Promise<void>;
  /** Saves the whole config; rejects with the backend's reason when it is refused. */
  save: (config: AppConfig) => Promise<void>;
  apply: (snapshot: ConfigSnapshot) => void;
}

export const useConfigStore = create<ConfigState>((set) => ({
  snapshot: null,

  async load() {
    set({ snapshot: await unwrap(commands.getConfig()) });
  },

  async save(config) {
    set({ snapshot: await unwrap(commands.updateConfig(config)) });
  },

  apply: (snapshot) => set({ snapshot }),
}));

export function useTerminalConfig(): TerminalConfig {
  return useConfigStore(
    (state) => state.snapshot?.config.terminal ?? DEFAULT_TERMINAL_CONFIG,
  );
}

/** "terminal.fontSize: must be between 8 and 32" reads better as "must be between 8 and 32". */
export function friendlyConfigError(message: string): string {
  const separator = message.indexOf(": ");
  return separator === -1 ? message : message.slice(separator + 2);
}
