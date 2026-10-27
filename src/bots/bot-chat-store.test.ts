import { beforeEach, describe, expect, it, vi } from "vitest";

import type { ChatMessage } from "@/bindings";

const { botsConversation, botsChat, botsStop, channelsStop } = vi.hoisted(
  () => ({
    botsConversation: vi.fn(),
    botsChat: vi.fn(),
    botsStop: vi.fn(),
    channelsStop: vi.fn(),
  }),
);

vi.mock("@/bindings", () => ({
  commands: {
    botsConversation,
    botsChat,
    botsStop,
    channelsStop,
    channelsConversation: vi.fn(),
  },
}));

import {
  botAnswering,
  botThread,
  useBotChatStore,
} from "@/bots/bot-chat-store";

const THREAD = botThread("ada", "t1");

/** A message the backend saved, with its real id. */
function saved(
  id: string,
  text: string,
  author: ChatMessage["author"],
): ChatMessage {
  return { id, author, text, at: 1, speaker: null };
}

/** A message the chat showed before the backend saved it. */
function local(
  id: string,
  text: string,
  author: ChatMessage["author"],
): ChatMessage {
  return saved(`local-${id}`, text, author);
}

function showing(messages: ChatMessage[]) {
  useBotChatStore.setState({
    threads: {
      [THREAD]: {
        messages,
        thinking: false,
        streaming: "",
        streamingAuthor: null,
        status: null,
      },
    },
  });
}

function texts(): string[] {
  return (useBotChatStore.getState().threads[THREAD]?.messages ?? []).map(
    (message) => message.text,
  );
}

describe("loading a conversation that is already on screen", () => {
  beforeEach(() => {
    useBotChatStore.setState({ threads: {} });
    botsConversation.mockReset();
  });

  it("shows each message once: the saved copy replaces the one shown before", async () => {
    botsConversation.mockResolvedValue({
      status: "ok",
      data: [saved("u1", "Olá", "user"), saved("b1", "Oi!", "bot")],
    });
    showing([local("1", "Olá", "user"), local("2", "Oi!", "bot")]);

    await useBotChatStore.getState().load(THREAD);

    expect(texts()).toEqual(["Olá", "Oi!"]);
  });

  it("keeps a message that arrived from an event while the list was loading", async () => {
    botsConversation.mockResolvedValue({
      status: "ok",
      data: [saved("u1", "Olá", "user")],
    });
    showing([saved("ev-1", "Pedido de acesso", "bot")]);

    await useBotChatStore.getState().load(THREAD);

    expect(texts()).toEqual(["Olá", "Pedido de acesso"]);
  });
});

describe("sending to a bot", () => {
  beforeEach(() => {
    useBotChatStore.setState({ threads: {} });
    botsConversation.mockReset();
    botsChat.mockReset();
  });

  it("shows the conversation as the backend saved it, with a request below the reply it follows", async () => {
    botsChat.mockResolvedValue({
      status: "ok",
      data: { ok: true, text: "Olá" },
    });
    botsConversation.mockResolvedValue({
      status: "ok",
      data: [
        saved("u1", "oi", "user"),
        saved("b1", "Olá", "bot"),
        saved("r1", "Preciso ler o projeto.", "bot"),
      ],
    });

    await useBotChatStore.getState().send(THREAD, "oi");

    expect(texts()).toEqual(["oi", "Olá", "Preciso ler o projeto."]);
  });
});

describe("stopping an answer", () => {
  beforeEach(() => {
    botsStop.mockReset().mockResolvedValue({ status: "ok", data: true });
    channelsStop.mockReset().mockResolvedValue({ status: "ok", data: true });
  });

  it("stops a bot in the thread that is answering", async () => {
    await useBotChatStore.getState().stop(THREAD);

    expect(botsStop).toHaveBeenCalledWith("ada", "t1");
    expect(channelsStop).not.toHaveBeenCalled();
  });

  it("stops a channel by the channel", async () => {
    await useBotChatStore.getState().stop("channel:dev");

    expect(channelsStop).toHaveBeenCalledWith("dev");
    expect(botsStop).not.toHaveBeenCalled();
  });
});

describe("a bot answers in one thread at a time", () => {
  it("knows a bot is answering when any of its threads is, and no other bot", () => {
    const answering = {
      messages: [],
      thinking: true,
      streaming: "",
      streamingAuthor: null,
      status: null,
    };
    useBotChatStore.setState({
      threads: { [botThread("ada", "t2")]: answering },
    });

    const state = useBotChatStore.getState();
    expect(botAnswering(state, "ada")).toBe(true);
    expect(botAnswering(state, "bento")).toBe(false);
  });

  it("forgets every thread of a deleted bot and keeps the others", () => {
    const empty = {
      messages: [],
      thinking: false,
      streaming: "",
      streamingAuthor: null,
      status: null,
    };
    useBotChatStore.setState({
      threads: {
        [botThread("ada", "t1")]: empty,
        [botThread("ada", "t2")]: empty,
        [botThread("bento", "t1")]: empty,
      },
    });

    useBotChatStore.getState().forgetBot("ada");

    expect(Object.keys(useBotChatStore.getState().threads)).toEqual([
      botThread("bento", "t1"),
    ]);
  });
});
