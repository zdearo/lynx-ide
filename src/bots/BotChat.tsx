import { useEffect } from "react";
import { toast } from "sonner";

import type { BotSummary } from "@/bindings";
import { botThread, useBotChatStore } from "@/bots/bot-chat-store";
import { shownThreadOf, useBotsStore } from "@/bots/bots-store";
import { Conversation } from "@/bots/Conversation";
import { ThreadSidebar } from "@/bots/ThreadSidebar";
import { THREADS_DOCKED_QUERY } from "@/hooks/layout-queries";
import { useMediaQuery } from "@/hooks/use-media-query";
import { errorMessage } from "@/lib/error-message";
import { copy } from "@/ui-copy";

/** The conversation with one bot across the main area: the thread picked in the list. */
export function BotChat({ bot }: { bot: BotSummary }) {
  const text = copy.bots.chat;
  const loadThreads = useBotsStore((state) => state.loadThreads);
  const open = useBotsStore((state) => state.threadsOpen);
  const toggleThreads = useBotsStore((state) => state.toggleThreads);
  // Beside the chat when the window is wide; over it, closing once a thread is picked, when it is not.
  const docked = useMediaQuery(THREADS_DOCKED_QUERY);
  const thread = useBotsStore((state) => shownThreadOf(state, bot.id));
  const key = thread ? botThread(bot.id, thread) : null;
  // Whether the bot is writing here. When it stops, the list is read again: the first message of a
  // thread has just given it its title, and the thread is now the most recently active.
  const writing = useBotChatStore((state) =>
    key === null ? false : (state.threads[key]?.thinking ?? false),
  );

  useEffect(() => {
    if (writing) return;
    loadThreads(bot.id).catch((error: unknown) =>
      toast.error(copy.bots.threads.loadFailed, {
        description: errorMessage(error),
      }),
    );
  }, [bot.id, loadThreads, writing]);

  if (key === null) return null;
  return (
    <div className="relative flex h-full min-h-0">
      {open && (
        <ThreadSidebar
          bot={bot}
          onPicked={docked ? undefined : toggleThreads}
          className={
            docked
              ? "w-52 shrink-0 border-r border-border/50"
              : "absolute inset-y-0 left-0 z-10 w-60 border-r border-border shadow-lg"
          }
        />
      )}
      <div className="h-full min-w-0 flex-1">
        <Conversation
          key={key}
          thread={key}
          label={text.log(bot.name)}
          placeholder={text.placeholder(bot.name)}
          empty={text.empty(bot.name)}
          loadFailed={text.loadFailed}
          botOf={() => bot}
        />
      </div>
    </div>
  );
}
