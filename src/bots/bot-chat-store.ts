import { create } from "zustand";

import { commands, type ChatMessage } from "@/bindings";
import { errorMessage } from "@/lib/error-message";
import { unwrap } from "@/lib/ipc-result";

/** The store key of one thread of a bot. Keys keep bots and channels apart even when their ids match. */
export const botThread = (bot: string, thread: string) =>
  `bot:${bot}:${thread}`;
/** The store key of a channel. */
export const channelThread = (id: string) => `channel:${id}`;

const isChannel = (thread: string) => thread.startsWith("channel:");
const idOf = (thread: string) => thread.slice(thread.indexOf(":") + 1);
/** The bot and the thread of a bot's key, or null for a channel's. */
const botAndThread = (key: string): { bot: string; thread: string } | null => {
  const [kind, bot, thread] = key.split(":");
  return kind === "bot" && bot && thread ? { bot, thread } : null;
};

/** Where a channel's answer stands. `author` is the member writing, `waiting` the ones after it. */
export interface AnswerStatus {
  phase: string;
  author: string | null;
  waiting: string[];
}

/** One conversation, as the chat screen holds it. */
export interface Thread {
  /** Null until the conversation has been loaded once. */
  messages: ChatMessage[] | null;
  /** The answer is not finished yet. */
  thinking: boolean;
  /** The reply's text so far, and the bot writing it. In a channel one bot writes at a time. */
  streaming: string;
  streamingAuthor: string | null;
  /** A channel's answer status. Null in a bot's own chat, and before the first status arrives. */
  status: AnswerStatus | null;
}

interface ChatState {
  threads: Record<string, Thread>;
  /** Throws when the conversation cannot be read; the screen says so. */
  load: (thread: string) => Promise<void>;
  send: (thread: string, text: string) => Promise<void>;
  /** Approves or denies a bot's request. Approving wakes the bot to go on. */
  decideAccess: (
    thread: string,
    bot: string,
    messageId: string,
    approve: boolean,
  ) => Promise<void>;
  /** Stops the answer being written in a thread. What was written stays, marked as stopped. */
  stop: (thread: string) => Promise<void>;
  /** A piece of a reply arrived. Pieces that come after the reply is complete are dropped. */
  delta: (thread: string, author: string, piece: string) => void;
  /** A channel's answer changed status. */
  status: (thread: string, status: AnswerStatus) => void;
  /** A message was saved to a conversation, so it is shown as soon as it exists. */
  added: (thread: string, message: ChatMessage) => void;
  /** Drops a conversation from memory, once its bot or thread is deleted. */
  forget: (thread: string) => void;
  /** Drops every conversation of a bot from memory, once the bot is deleted. */
  forgetBot: (bot: string) => void;
}

const NO_THREAD: Thread = {
  messages: null,
  thinking: false,
  streaming: "",
  streamingAuthor: null,
  status: null,
};

export function threadOf(state: ChatState, thread: string): Thread {
  return state.threads[thread] ?? NO_THREAD;
}

/** Whether a bot is answering in any of its threads. It answers in one at a time. */
export function botAnswering(state: ChatState, bot: string): boolean {
  const prefix = `bot:${bot}:`;
  return Object.entries(state.threads).some(
    ([key, thread]) => key.startsWith(prefix) && thread.thinking,
  );
}

// Messages shown before the backend has saved them get a local id, until the next load.
let localId = 0;

export const useBotChatStore = create<ChatState>((set, get) => {
  const patch = (thread: string, change: Partial<Thread>) =>
    set((state) => ({
      threads: {
        ...state.threads,
        [thread]: { ...threadOf(state, thread), ...change },
      },
    }));

  const add = (thread: string, author: ChatMessage["author"], text: string) => {
    localId += 1;
    const message: ChatMessage = {
      id: `local-${localId}`,
      author,
      text,
      at: Math.floor(Date.now() / 1000),
      speaker: null,
    };
    const messages = threadOf(get(), thread).messages ?? [];
    patch(thread, { messages: [...messages, message] });
  };

  return {
    threads: {},

    async load(thread) {
      const loaded = await unwrap(
        isChannel(thread)
          ? commands.channelsConversation(idOf(thread))
          : commands.botsConversation(
              botAndThread(thread)?.bot ?? "",
              botAndThread(thread)?.thread ?? "",
            ),
      );
      // Messages that arrived while the list was loading are kept, after the loaded ones. A local message
      // (shown before the backend saved it) is never kept: its saved copy is already in `loaded`.
      const current = threadOf(get(), thread).messages ?? [];
      const extra = current.filter(
        (message) =>
          !message.id.startsWith("local-") &&
          !loaded.some((saved) => saved.id === message.id),
      );
      patch(thread, { messages: [...loaded, ...extra] });
    },

    async send(thread, text) {
      if (threadOf(get(), thread).thinking) return;
      const sent = text.trim();
      patch(thread, {
        thinking: true,
        streaming: "",
        streamingAuthor: null,
        status: null,
      });
      try {
        if (isChannel(thread)) {
          // The channel saves and shows each message as it happens, through events.
          await unwrap(commands.channelsChat(idOf(thread), sent));
        } else {
          const key = botAndThread(thread);
          if (!key) return;
          add(thread, "user", sent);
          await unwrap(commands.botsChat(key.bot, key.thread, sent));
          // The saved conversation has the reply under the message it answers, after any request the bot
          // made during its turn, so the screen takes the order from the backend.
          await get().load(thread);
        }
      } catch (error) {
        add(thread, "error", errorMessage(error));
      } finally {
        patch(thread, {
          thinking: false,
          streaming: "",
          streamingAuthor: null,
          status: null,
        });
      }
    },

    async stop(thread) {
      const key = botAndThread(thread);
      await unwrap(
        key
          ? commands.botsStop(key.bot, key.thread)
          : commands.channelsStop(idOf(thread)),
      );
    },

    async decideAccess(thread, bot, messageId, approve) {
      const id = botAndThread(thread)?.thread ?? "";
      if (!approve) {
        try {
          await unwrap(commands.botsDenyAccess(bot, id, messageId));
        } finally {
          await get().load(thread);
        }
        return;
      }
      if (threadOf(get(), thread).thinking) return;
      // Approving wakes the bot, so its answer streams in like any other reply.
      patch(thread, {
        thinking: true,
        streaming: "",
        streamingAuthor: null,
        status: null,
      });
      try {
        await unwrap(commands.botsApproveAccess(bot, id, messageId));
      } finally {
        patch(thread, {
          thinking: false,
          streaming: "",
          streamingAuthor: null,
          status: null,
        });
        await get().load(thread);
      }
    },

    delta(thread, author, piece) {
      const current = threadOf(get(), thread);
      if (!current.thinking) return;
      const streaming =
        current.streamingAuthor === author ? current.streaming + piece : piece;
      patch(thread, { streaming, streamingAuthor: author });
    },

    status(thread, status) {
      patch(thread, { status });
    },

    added(thread, message) {
      const current = threadOf(get(), thread);
      const messages = current.messages ?? [];
      if (messages.some((existing) => existing.id === message.id)) return;
      // A saved reply replaces the text that was streaming for its bot.
      const finishes =
        message.author !== "user" &&
        current.streamingAuthor === (message.speaker ?? null);
      patch(thread, {
        messages: [...messages, message],
        ...(finishes ? { streaming: "", streamingAuthor: null } : {}),
      });
    },

    forget(thread) {
      set((state) => {
        const threads = { ...state.threads };
        delete threads[thread];
        return { threads };
      });
    },

    forgetBot(bot) {
      const prefix = `bot:${bot}:`;
      set((state) => ({
        threads: Object.fromEntries(
          Object.entries(state.threads).filter(
            ([key]) => !key.startsWith(prefix),
          ),
        ),
      }));
    },
  };
});
