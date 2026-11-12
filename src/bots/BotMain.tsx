import type { BotSummary, ChannelSummary } from "@/bindings";
import { BotAvatar } from "@/bots/BotAvatar";
import { BotChat } from "@/bots/BotChat";
import { ChannelChat } from "@/bots/ChannelChat";
import {
  selectedBotOf,
  selectedChannelOf,
  useBotsStore,
} from "@/bots/bots-store";
import { ChatsIcon, SidebarSimpleIcon } from "@phosphor-icons/react";
import { TooltipIconButton } from "@/components/TooltipIconButton";
import { shortcutLabel } from "@/platform-shortcuts";
import { copy } from "@/ui-copy";

/** The bar above the conversation: the two panel toggles, and the bot or channel it belongs to. */
export function BotTopBar({
  bot,
  channel,
  rightPanelOpen,
  onToggleSidebar,
  onToggleRightPanel,
}: {
  bot: BotSummary | null;
  channel: ChannelSummary | null;
  rightPanelOpen: boolean;
  onToggleSidebar: () => void;
  onToggleRightPanel: () => void;
}) {
  const threadsOpen = useBotsStore((state) => state.threadsOpen);
  const toggleThreads = useBotsStore((state) => state.toggleThreads);
  return (
    <div className="flex h-9 shrink-0 items-center gap-2 px-2">
      <TooltipIconButton
        label={copy.sidebar.toggle}
        shortcut={shortcutLabel("toggleSidebar")}
        onClick={onToggleSidebar}
      >
        <SidebarSimpleIcon />
      </TooltipIconButton>
      {bot && (
        <TooltipIconButton
          label={threadsOpen ? copy.bots.threads.hide : copy.bots.threads.show}
          aria-pressed={threadsOpen}
          onClick={toggleThreads}
        >
          <ChatsIcon />
        </TooltipIconButton>
      )}
      {bot && (
        <div className="flex min-w-0 items-center gap-2">
          <BotAvatar name={bot.name} color={bot.color} />
          <span className="truncate text-ui">{bot.name}</span>
        </div>
      )}
      {channel && (
        <div className="flex min-w-0 items-center gap-1.5">
          <span aria-hidden="true" className="text-muted-foreground">
            #
          </span>
          <span className="truncate text-ui">{channel.name}</span>
        </div>
      )}
      <div className="ml-auto">
        <TooltipIconButton
          label={copy.rightPanel.toggle}
          shortcut={shortcutLabel("toggleFilesPanel")}
          aria-pressed={rightPanelOpen}
          onClick={onToggleRightPanel}
        >
          <SidebarSimpleIcon mirrored />
        </TooltipIconButton>
      </div>
    </div>
  );
}

/** What the main area shows in the Bots view: the picked channel or bot, or a hint to pick one. */
export function BotArea() {
  const bot = useBotsStore(selectedBotOf);
  const channel = useBotsStore(selectedChannelOf);
  const bots = useBotsStore((state) => state.bots);
  if (channel) {
    return (
      <div className="absolute inset-0 bg-background">
        <ChannelChat key={channel.id} channel={channel} bots={bots ?? []} />
      </div>
    );
  }
  if (!bot) {
    return (
      <div className="absolute inset-0 flex items-center justify-center px-6 text-center text-ui text-muted-foreground">
        {copy.bots.chat.pickBot}
      </div>
    );
  }
  return (
    <div className="absolute inset-0 bg-background">
      <BotChat key={bot.id} bot={bot} />
    </div>
  );
}
