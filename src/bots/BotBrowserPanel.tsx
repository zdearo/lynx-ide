import { GlobeIcon } from "@phosphor-icons/react";
import { selectedBotOf, useBotsStore } from "@/bots/bots-store";
import { copy } from "@/ui-copy";

/** The right panel's browser tab for a bot. The bot's own browser is not wired up yet, so this only shows where it will be. */
export function BotBrowserPanel() {
  const bot = useBotsStore(selectedBotOf);
  if (!bot) {
    return (
      <p className="p-4 text-center text-ui text-muted-foreground">
        {copy.bots.chat.pickBot}
      </p>
    );
  }
  return (
    <div className="flex h-full flex-col items-center justify-center gap-2 px-6 text-center">
      <GlobeIcon className="size-5 text-muted-foreground" />
      <h2 className="text-ui font-medium">
        {copy.bots.browser.title(bot.name)}
      </h2>
      <p className="max-w-xs text-meta text-muted-foreground">
        {copy.bots.browser.body}
      </p>
    </div>
  );
}
