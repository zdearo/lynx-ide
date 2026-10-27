import { create } from "zustand";

import { commands, type ChatGptStatus } from "@/bindings";
import { unwrap } from "@/lib/ipc-result";

interface ChatGptAuthState {
  /** Null until the backend sends the first status. */
  status: ChatGptStatus | null;
  load: () => Promise<void>;
  /** Takes a status the backend sent on its own, when an attempt moved. */
  apply: (status: ChatGptStatus) => void;
  signIn: () => Promise<void>;
  signOut: () => Promise<void>;
}

/** The ChatGPT sign-in as the backend reports it. Every action goes through the backend. */
export const useChatGptAuthStore = create<ChatGptAuthState>((set) => ({
  status: null,

  async load() {
    set({ status: await commands.botsChatgptStatus() });
  },

  apply(status) {
    set({ status });
  },

  async signIn() {
    set({ status: await commands.botsChatgptSignIn() });
  },

  async signOut() {
    set({ status: await unwrap(commands.botsChatgptSignOut()) });
  },
}));
