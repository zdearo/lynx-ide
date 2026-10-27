import { useEffect, useState, type ReactNode } from "react";
import { toast } from "sonner";

import { BotAvatar } from "@/bots/BotAvatar";
import { ChannelDialog } from "@/bots/ChannelDialog";
import { useBotsStore } from "@/bots/bots-store";
import { PlusGlyph18 } from "@/components/icons/PlusGlyph18";
import { TooltipIconButton } from "@/components/TooltipIconButton";
import { ScrollArea } from "@/components/ui/scroll-area";
import { useDialogStore } from "@/dialog-store";
import { errorMessage } from "@/lib/error-message";
import { cn } from "@/lib/utils";
import { copy } from "@/ui-copy";

const row = (selected: boolean) =>
  cn(
    "flex w-full min-w-0 items-center gap-2.5 rounded-md px-2 py-1.5 text-left outline-none hover:bg-sidebar-accent focus-visible:ring-2 focus-visible:ring-ring",
    selected && "bg-sidebar-accent",
  );

/** The Bots view: the channels to talk in, then the bots, each with its team beside it. */
export function BotsPanel() {
  const bots = useBotsStore((state) => state.bots);
  const channels = useBotsStore((state) => state.channels);
  const selectedId = useBotsStore((state) => state.selectedId);
  const selectedChannelId = useBotsStore((state) => state.selectedChannelId);
  const select = useBotsStore((state) => state.select);
  const selectChannel = useBotsStore((state) => state.selectChannel);
  const load = useBotsStore((state) => state.load);
  const loadChannels = useBotsStore((state) => state.loadChannels);
  const askBotWizard = useDialogStore((state) => state.askBotWizard);
  const [creating, setCreating] = useState(false);
  const text = copy.bots;
  const channelText = text.channels;

  useEffect(() => {
    load().catch((error: unknown) =>
      toast.error(text.loadFailed, { description: errorMessage(error) }),
    );
  }, [load, text.loadFailed]);

  useEffect(() => {
    loadChannels().catch((error: unknown) =>
      toast.error(channelText.loadFailed, { description: errorMessage(error) }),
    );
  }, [loadChannels, channelText.loadFailed]);

  // Bots of one team sit together; a bot with no team comes first.
  const sorted = bots
    ?.slice()
    .sort(
      (a, b) =>
        (a.team ?? "").localeCompare(b.team ?? "") ||
        a.name.localeCompare(b.name),
    );

  return (
    <div className="flex h-full flex-col bg-sidebar text-sidebar-foreground">
      <ScrollArea className="min-h-0 flex-1 [&>div]:block!">
        <Section
          title={channelText.title}
          action={
            <TooltipIconButton
              label={channelText.newChannel}
              onClick={() => setCreating(true)}
            >
              <PlusGlyph18 />
            </TooltipIconButton>
          }
        >
          {channels !== null && channels.length === 0 && (
            <p className="px-3 pb-2 text-meta text-muted-foreground">
              {channelText.none}
            </p>
          )}
          <ul
            role="list"
            aria-label={channelText.title}
            className="grid grid-cols-1 gap-0.5 px-2 pb-3"
          >
            {(channels ?? []).map((channel) => {
              const selected = channel.id === selectedChannelId;
              return (
                <li key={channel.id}>
                  <button
                    type="button"
                    aria-current={selected ? "true" : undefined}
                    onClick={() => selectChannel(channel.id)}
                    className={row(selected)}
                  >
                    <span aria-hidden="true" className="text-muted-foreground">
                      #
                    </span>
                    <span className="min-w-0 truncate text-ui">
                      {channel.name}
                    </span>
                  </button>
                </li>
              );
            })}
          </ul>
        </Section>

        <Section
          title={text.title}
          action={
            <TooltipIconButton
              label={text.newBot}
              onClick={() => askBotWizard(true)}
            >
              <PlusGlyph18 />
            </TooltipIconButton>
          }
        >
          {bots !== null && bots.length === 0 && (
            <p className="px-3 pb-2 text-meta text-muted-foreground">
              {text.none}
            </p>
          )}
          {sorted && sorted.length > 0 && (
            <ul
              role="list"
              aria-label={text.title}
              className="grid grid-cols-1 gap-0.5 px-2 pb-3"
            >
              {sorted.map((bot) => (
                <li key={bot.id}>
                  <button
                    type="button"
                    aria-current={bot.id === selectedId ? "true" : undefined}
                    onClick={() => select(bot.id)}
                    className={row(bot.id === selectedId)}
                  >
                    <BotAvatar name={bot.name} color={bot.color} />
                    <span className="min-w-0 flex-1 truncate text-ui">
                      {bot.name}
                    </span>
                    {bot.team && (
                      <span className="shrink-0 truncate text-meta text-muted-foreground">
                        {bot.team}
                      </span>
                    )}
                  </button>
                </li>
              ))}
            </ul>
          )}
        </Section>
      </ScrollArea>

      {creating && bots !== null && (
        <ChannelDialog bots={bots} onClose={() => setCreating(false)} />
      )}
    </div>
  );
}

/** A labelled group of the panel: a heading with one action beside it, then its rows. */
function Section({
  title,
  action,
  children,
}: {
  title: string;
  action: ReactNode;
  children: ReactNode;
}) {
  return (
    <div className="pt-3">
      <div className="flex h-7 items-center justify-between pr-2 pl-3">
        <h2 className="text-xs font-medium text-muted-foreground">{title}</h2>
        {action}
      </div>
      {children}
    </div>
  );
}
