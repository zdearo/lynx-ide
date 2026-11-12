import { useEffect, useRef, useState, type KeyboardEvent } from "react";
import { toast } from "sonner";

import type { AccessRequest, BotSummary, ChatMessage } from "@/bindings";
import { CopyButton } from "@/bots/CopyButton";
import { BotAvatar } from "@/bots/BotAvatar";
import { BotMarkdown } from "@/bots/BotMarkdown";
import {
  threadOf,
  useBotChatStore,
  type AnswerStatus,
} from "@/bots/bot-chat-store";
import { ArrowUpIcon, StopIcon } from "@phosphor-icons/react";
import { TooltipIconButton } from "@/components/TooltipIconButton";
import { Button } from "@/components/ui/button";
import { Textarea } from "@/components/ui/textarea";
import { errorMessage } from "@/lib/error-message";
import { copy } from "@/ui-copy";

/** How close to the bottom, in pixels, still counts as reading the newest messages. */
const AT_BOTTOM_PX = 32;

const TIME = new Intl.DateTimeFormat(undefined, {
  hour: "2-digit",
  minute: "2-digit",
});

/** Stands in for a speaker the sidebar no longer lists, so its messages still read. */
const UNKNOWN_BOT: BotSummary = {
  id: "",
  name: copy.bots.chat.bot,
  color: "graphite",
  provider: "claude",
  model: null,
  team: null,
};

interface ConversationProps {
  /** The store key of the conversation: a bot's own chat, or a channel. */
  thread: string;
  label: string;
  placeholder: string;
  empty: string;
  loadFailed: string;
  /** Who wrote a bot message, by its speaker. Null for a reply nobody is named on yet. */
  botOf: (speaker: string | null) => BotSummary | null;
  /** Why the box cannot be written in right now, shown in its place. Absent: it can. */
  blocked?: string;
}

/**
 * A conversation across the main area: what was said, the reply being written, and the box to write
 * in. The newest message sits just above the box, and older ones scroll up.
 */
export function Conversation({
  thread: key,
  label,
  placeholder,
  empty,
  loadFailed,
  botOf,
  blocked,
}: ConversationProps) {
  const thread = useBotChatStore((state) => threadOf(state, key));
  const load = useBotChatStore((state) => state.load);
  const send = useBotChatStore((state) => state.send);
  const decideAccess = useBotChatStore((state) => state.decideAccess);
  const stop = useBotChatStore((state) => state.stop);
  const [draft, setDraft] = useState("");
  const endRef = useRef<HTMLDivElement>(null);
  // Whether the view sits at the bottom. A reply follows only then, so older messages are not pulled away.
  const followRef = useRef(true);
  const text = copy.bots.chat;
  const messages = thread.messages ?? [];
  const status = thread.status;
  // The member writing the reply in progress. A channel that is still choosing has none yet.
  const writer = thread.streamingAuthor ?? status?.author ?? null;
  const replying =
    thread.thinking &&
    (status === null ||
      status.phase === "answering" ||
      thread.streaming !== "");
  const statusLine = thread.thinking && status ? statusText(status, botOf) : "";

  useEffect(() => {
    load(key).catch((error: unknown) =>
      toast.error(loadFailed, { description: errorMessage(error) }),
    );
  }, [key, load, loadFailed]);

  // A channel shows its reply row only once a member is answering, so the row's appearance counts too.
  useEffect(() => {
    if (followRef.current) endRef.current?.scrollIntoView({ block: "end" });
  }, [messages.length, replying, thread.streaming.length]);

  function submit() {
    const message = draft.trim();
    if (!message || thread.thinking || blocked) return;
    // Sending shows the new message, even when the view had been scrolled up.
    followRef.current = true;
    setDraft("");
    void send(key, message);
  }

  function stopAnswer() {
    stop(key).catch((error: unknown) =>
      toast.error(text.stopFailed, { description: errorMessage(error) }),
    );
  }

  function decide(bot: string, messageId: string, approve: boolean) {
    decideAccess(key, bot, messageId, approve).catch((error: unknown) =>
      toast.error(copy.bots.access.failed, {
        description: errorMessage(error),
      }),
    );
  }

  function onKeyDown(event: KeyboardEvent<HTMLTextAreaElement>) {
    // Enter sends; Shift+Enter starts a new line, and Enter during an IME composition is not a send.
    if (
      event.key === "Enter" &&
      !event.shiftKey &&
      !event.nativeEvent.isComposing
    ) {
      event.preventDefault();
      submit();
    }
  }

  return (
    <div className="flex h-full min-h-0 flex-col">
      <div
        className="min-h-0 flex-1 overflow-y-auto"
        onScroll={(event) => {
          const { scrollTop, scrollHeight, clientHeight } = event.currentTarget;
          followRef.current =
            scrollHeight - scrollTop - clientHeight < AT_BOTTOM_PX;
        }}
      >
        <div className="flex min-h-full flex-col justify-end">
          <div
            role="log"
            aria-label={label}
            className="flex flex-col gap-5 px-6 py-6"
          >
            {thread.messages !== null && messages.length === 0 && (
              <p className="py-10 text-center text-ui text-muted-foreground">
                {empty}
              </p>
            )}
            {messages.map((message) => {
              const bot = botOf(message.speaker ?? null) ?? UNKNOWN_BOT;
              return (
                <Message
                  key={message.id}
                  message={message}
                  bot={bot}
                  onDecide={(messageId, approve) =>
                    decide(bot.id, messageId, approve)
                  }
                />
              );
            })}
            {replying && (
              <Reply bot={botOf(writer)} streaming={thread.streaming} />
            )}
          </div>
        </div>
        <div ref={endRef} />
      </div>

      <div
        role="status"
        aria-live="polite"
        className="min-h-5 px-6 text-meta text-muted-foreground"
      >
        {statusLine}
      </div>

      <form
        onSubmit={(event) => {
          event.preventDefault();
          submit();
        }}
        className="px-6 pt-2 pb-4"
      >
        <div className="flex items-end gap-2 rounded-xl border border-input bg-muted/40 p-2 focus-within:ring-2 focus-within:ring-ring">
          <Textarea
            value={draft}
            rows={1}
            aria-label={placeholder}
            placeholder={blocked ?? placeholder}
            disabled={blocked !== undefined}
            onChange={(event) => setDraft(event.target.value)}
            onKeyDown={onKeyDown}
            className="max-h-40 min-h-8 flex-1 resize-none border-0 bg-transparent px-2 py-1.5 shadow-none focus-visible:ring-0 dark:bg-transparent"
          />
          {thread.thinking ? (
            <TooltipIconButton
              label={text.stop}
              variant="default"
              size="icon-sm"
              className="rounded-full"
              onClick={stopAnswer}
            >
              <StopIcon weight="fill" />
            </TooltipIconButton>
          ) : (
            <TooltipIconButton
              label={text.send}
              variant="default"
              size="icon-sm"
              className="rounded-full"
              disabled={!draft.trim() || blocked !== undefined}
              onClick={submit}
            >
              <ArrowUpIcon />
            </TooltipIconButton>
          )}
        </div>
      </form>
    </div>
  );
}

/** What the status line says while a channel answers: who is choosing, who is writing, who is next. */
function statusText(
  status: AnswerStatus,
  botOf: (speaker: string | null) => BotSummary | null,
): string {
  const channel = copy.bots.channels;
  const nameOf = (id: string) => botOf(id)?.name ?? id;
  if (status.phase === "routing") return channel.routing;
  if (status.phase !== "answering" || status.author === null) return "";
  const writing = channel.writing(nameOf(status.author));
  if (status.waiting.length === 0) return writing;
  return `${writing} · ${channel.then(status.waiting.map(nameOf).join(", "))}`;
}

/** One finished message: who wrote it, when, and the text. The bot's words are Markdown. */
function Message({
  message,
  bot,
  onDecide,
}: {
  message: ChatMessage;
  bot: BotSummary;
  onDecide: (messageId: string, approve: boolean) => void;
}) {
  const text = copy.bots.chat;
  if (message.author === "error") {
    return (
      <p role="alert" className="pl-9 text-meta text-destructive">
        {text.failed}: {message.text}
      </p>
    );
  }
  const mine = message.author === "user";
  const when = new Date(message.at * 1000);
  return (
    <article className="group flex items-start gap-3">
      {mine ? (
        <span
          aria-hidden="true"
          className="inline-flex size-6 shrink-0 items-center justify-center rounded-md bg-muted text-meta font-medium text-foreground"
        >
          {text.you.charAt(0)}
        </span>
      ) : (
        <BotAvatar name={bot.name} color={bot.color} />
      )}
      <div className="min-w-0 flex-1">
        <div className="flex items-baseline gap-2">
          <span className="text-ui font-medium">
            {mine ? text.you : bot.name}
          </span>
          <time
            dateTime={when.toISOString()}
            className="text-meta text-muted-foreground tabular-nums"
          >
            {TIME.format(when)}
          </time>
          {!message.request && (
            <CopyButton
              text={message.text}
              label={text.copyMessage}
              className="ml-auto"
            />
          )}
        </div>
        <div className="mt-0.5 text-ui break-words">
          {mine ? (
            <p className="whitespace-pre-wrap">{message.text}</p>
          ) : message.request ? (
            <AccessRequestCard
              request={message.request}
              reason={message.text}
              onDecide={(approve) => onDecide(message.id, approve)}
            />
          ) : (
            <BotMarkdown>{message.text}</BotMarkdown>
          )}
          {message.stopped && (
            <p className="mt-1 text-meta text-muted-foreground">
              {text.stopped}
            </p>
          )}
        </div>
      </div>
    </article>
  );
}

/**
 * The reply while it is being written: the bot's name, then the text so far. Until the first piece
 * arrives, the working status stands in for it.
 */
function Reply({
  bot,
  streaming,
}: {
  bot: BotSummary | null;
  streaming: string;
}) {
  const text = copy.bots.chat;
  return (
    <article className="flex items-start gap-3" aria-busy="true">
      {bot && <BotAvatar name={bot.name} color={bot.color} />}
      <div className={bot ? "min-w-0 flex-1" : "min-w-0 flex-1 pl-9"}>
        {bot && <div className="text-ui font-medium">{bot.name}</div>}
        {streaming ? (
          <div className="mt-0.5 text-ui break-words">
            <BotMarkdown>{streaming}</BotMarkdown>
            <span
              aria-hidden="true"
              className="ml-0.5 inline-block h-4 w-px animate-pulse bg-foreground align-middle motion-reduce:animate-none"
            />
          </div>
        ) : (
          <span
            role="status"
            className="mt-1 inline-flex items-center gap-2 rounded-md border border-border bg-muted px-3 py-1 text-meta text-muted-foreground"
          >
            {text.working}
            <ThinkingDots />
          </span>
        )}
      </div>
    </article>
  );
}

/**
 * A bot asks for a permission. The card is the same for every kind: a heading for the kind, what is
 * asked as a list, and the reason. The person approves or refuses it here; the answer is kept in the
 * conversation, so the card still shows it after a reload.
 */
function AccessRequestCard({
  request,
  reason,
  onDecide,
}: {
  request: AccessRequest;
  reason: string;
  onDecide: (approve: boolean) => void;
}) {
  const text = copy.bots.access;
  const kind = request.kind ?? "project";
  const state = request.state ?? "pending";
  return (
    <div className="mt-1 grid gap-2 rounded-lg border border-border bg-muted/40 p-3">
      <p className="font-medium">{text.kinds[kind] ?? text.kindFallback}</p>
      <ul role="list" className="flex flex-wrap gap-1.5">
        {(request.items ?? []).map((item) => (
          <li
            key={item}
            className="rounded-md bg-muted px-2 py-0.5 font-mono text-meta break-all"
          >
            {item}
          </li>
        ))}
      </ul>
      <p className="whitespace-pre-wrap">{reason}</p>
      <p className="text-meta text-muted-foreground">{text.states[state]}</p>
      {state === "pending" && (
        <div className="flex gap-2">
          <Button size="sm" onClick={() => onDecide(true)}>
            {text.approve}
          </Button>
          <Button size="sm" variant="outline" onClick={() => onDecide(false)}>
            {text.deny}
          </Button>
        </div>
      )}
    </div>
  );
}

/** Three dots that pulse one after another; still when the system asks for less motion. */
function ThinkingDots() {
  return (
    <span aria-hidden="true" className="inline-flex gap-0.5">
      {[0, 1, 2].map((dot) => (
        <span
          key={dot}
          className="size-1 animate-pulse rounded-full bg-current motion-reduce:animate-none"
          style={{ animationDelay: `${dot * 150}ms` }}
        />
      ))}
    </span>
  );
}
