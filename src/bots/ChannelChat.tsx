import type { BotSummary, ChannelSummary } from "@/bindings";
import { channelThread } from "@/bots/bot-chat-store";
import { Conversation } from "@/bots/Conversation";
import { copy } from "@/ui-copy";

/** A channel across the main area. Each message names the member who wrote it. */
export function ChannelChat({
  channel,
  bots,
}: {
  channel: ChannelSummary;
  bots: BotSummary[];
}) {
  const text = copy.bots.channels;
  return (
    <Conversation
      thread={channelThread(channel.id)}
      label={text.log(channel.name)}
      placeholder={text.placeholder(channel.name)}
      empty={text.empty(channel.name)}
      loadFailed={text.loadFailed}
      botOf={(speaker) => bots.find((bot) => bot.id === speaker) ?? null}
    />
  );
}
