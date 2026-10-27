import { create } from "zustand";

import {
  commands,
  type BotDetail,
  type BotSetup,
  type BotSummary,
  type ChannelSummary,
  type NewBot,
  type NewChannel,
  type ThreadSummary,
} from "@/bindings";
import {
  botThread,
  channelThread,
  useBotChatStore,
} from "@/bots/bot-chat-store";
import { unwrap } from "@/lib/ipc-result";

interface BotsState {
  /** Null until the backend has listed the bots once. */
  bots: BotSummary[] | null;
  /** The bot the main area talks to. Null: none is picked, or the picked one is gone. */
  selectedId: string | null;
  /** Null until the backend has listed the channels once. */
  channels: ChannelSummary[] | null;
  /** The channel the main area talks to. Picking a bot clears it, and the other way round. */
  selectedChannelId: string | null;
  /** Each bot's threads, the most recently active first. A bot is missing until they are loaded. */
  threadsByBot: Record<string, ThreadSummary[]>;
  /** The thread each bot's chat shows. A bot with none picked shows its first. */
  selectedThreadByBot: Record<string, string>;
  /** Whether the column of a bot's threads is open beside its chat. Remembered between runs. */
  threadsOpen: boolean;
  toggleThreads: () => void;
  load: () => Promise<void>;
  loadChannels: () => Promise<void>;
  select: (id: string) => void;
  selectChannel: (id: string) => void;
  /** Makes a bot, reloads the list, and picks the new bot. */
  create: (bot: NewBot) => Promise<BotSummary>;
  /** Makes a channel, reloads the list, and picks it. */
  createChannel: (channel: NewChannel) => Promise<ChannelSummary>;
  /** Replaces a bot's instructions, and returns the bot as it now is. */
  updateInstructions: (id: string, instructions: string) => Promise<BotDetail>;
  /** Changes a bot's engine, model and team, and returns the bot as it now is. */
  updateSetup: (id: string, setup: BotSetup) => Promise<BotDetail>;
  /** Replaces a channel's members, reloads the channels, and returns the channel as it now is. */
  updateMembers: (id: string, members: string[]) => Promise<ChannelSummary>;
  /** Removes a bot with its conversation. If it was picked, nothing is picked any more. */
  remove: (id: string) => Promise<void>;
  /** Removes a channel with its conversation. The bots in it stay. If it was picked, nothing is picked any more. */
  removeChannel: (id: string) => Promise<void>;
  /** Reads a bot's threads. A bot with none gets its first, so there is always one to talk in. */
  loadThreads: (bot: string) => Promise<void>;
  /** Starts a thread with a bot, and shows it. */
  createThread: (bot: string) => Promise<ThreadSummary>;
  selectThread: (bot: string, thread: string) => void;
  renameThread: (bot: string, thread: string, title: string) => Promise<void>;
  /** Removes a thread. If it was the one shown, the bot's first is shown. */
  deleteThread: (bot: string, thread: string) => Promise<void>;
}

/** The thread a bot's chat shows: the picked one if it still exists, else the first. Null until loaded. */
export function shownThreadOf(
  state: Pick<BotsState, "threadsByBot" | "selectedThreadByBot">,
  bot: string,
): string | null {
  const threads = state.threadsByBot[bot];
  if (!threads || threads.length === 0) return null;
  const picked = state.selectedThreadByBot[bot];
  return threads.some((thread) => thread.id === picked)
    ? picked
    : threads[0].id;
}

const THREADS_OPEN_KEY = "lynx.bots.threadsOpen";

/** Open unless the person closed it; localStorage can be blocked, and then it is open. */
function readThreadsOpen(): boolean {
  try {
    return localStorage.getItem(THREADS_OPEN_KEY) !== "false";
  } catch {
    return true;
  }
}

/** The bot that is picked, or null. Returns the list's own object, so selecting it does not re-render. */
export function selectedBotOf(
  state: Pick<BotsState, "bots" | "selectedId">,
): BotSummary | null {
  return state.bots?.find((bot) => bot.id === state.selectedId) ?? null;
}

/** The channel that is picked, or null. */
export function selectedChannelOf(
  state: Pick<BotsState, "channels" | "selectedChannelId">,
): ChannelSummary | null {
  return (
    state.channels?.find((channel) => channel.id === state.selectedChannelId) ??
    null
  );
}

/** The bots and channels as the backend lists them. Every change goes through the backend. */
export const useBotsStore = create<BotsState>((set, get) => ({
  bots: null,
  selectedId: null,
  channels: null,
  selectedChannelId: null,
  threadsByBot: {},
  selectedThreadByBot: {},
  threadsOpen: readThreadsOpen(),
  toggleThreads() {
    const open = !get().threadsOpen;
    set({ threadsOpen: open });
    try {
      localStorage.setItem(THREADS_OPEN_KEY, String(open));
    } catch {
      // Not remembering is fine.
    }
  },

  async load() {
    const bots = await unwrap(commands.botsList());
    const { selectedId } = get();
    set({
      bots,
      selectedId: bots.some((bot) => bot.id === selectedId) ? selectedId : null,
    });
  },

  async loadChannels() {
    const channels = await unwrap(commands.channelsList());
    const { selectedChannelId } = get();
    set({
      channels,
      selectedChannelId: channels.some(
        (channel) => channel.id === selectedChannelId,
      )
        ? selectedChannelId
        : null,
    });
  },

  select(id) {
    set({ selectedId: id, selectedChannelId: null });
  },

  selectChannel(id) {
    set({ selectedChannelId: id, selectedId: null });
  },

  async create(bot) {
    const created = await unwrap(commands.botsCreate(bot));
    await get().load();
    set({ selectedId: created.id, selectedChannelId: null });
    return created;
  },

  async createChannel(channel) {
    const created = await unwrap(commands.channelsCreate(channel));
    await get().loadChannels();
    set({ selectedChannelId: created.id, selectedId: null });
    return created;
  },

  async updateSetup(id, setup) {
    const updated = await unwrap(commands.botsUpdateSetup(id, setup));
    await get().load();
    return updated;
  },

  async updateMembers(id, members) {
    const updated = await unwrap(commands.channelsUpdateMembers(id, members));
    await get().loadChannels();
    return updated;
  },

  updateInstructions(id, instructions) {
    return unwrap(commands.botsUpdateInstructions(id, instructions));
  },

  async remove(id) {
    await unwrap(commands.botsDelete(id));
    useBotChatStore.getState().forgetBot(id);
    set((state) => {
      const threadsByBot = { ...state.threadsByBot };
      delete threadsByBot[id];
      return { threadsByBot };
    });
    await get().load();
  },

  async loadThreads(bot) {
    const threads = await unwrap(commands.botsThreads(bot));
    set((state) => ({
      threadsByBot: { ...state.threadsByBot, [bot]: threads },
    }));
  },

  async createThread(bot) {
    const created = await unwrap(commands.botsThreadCreate(bot));
    await get().loadThreads(bot);
    get().selectThread(bot, created.id);
    return created;
  },

  selectThread(bot, thread) {
    set((state) => ({
      selectedThreadByBot: { ...state.selectedThreadByBot, [bot]: thread },
    }));
  },

  async renameThread(bot, thread, title) {
    await unwrap(commands.botsThreadRename(bot, thread, title));
    await get().loadThreads(bot);
  },

  async deleteThread(bot, thread) {
    await unwrap(commands.botsThreadDelete(bot, thread));
    useBotChatStore.getState().forget(botThread(bot, thread));
    await get().loadThreads(bot);
  },

  async removeChannel(id) {
    await unwrap(commands.channelsDelete(id));
    useBotChatStore.getState().forget(channelThread(id));
    await get().loadChannels();
  },
}));
