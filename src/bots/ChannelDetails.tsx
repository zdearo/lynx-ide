import { useState } from "react";
import { toast } from "sonner";

import type { ChannelSummary } from "@/bindings";
import { BotAvatar } from "@/bots/BotAvatar";
import {
  channelThread,
  threadOf,
  useBotChatStore,
} from "@/bots/bot-chat-store";
import { DetailRow, DetailSection } from "@/bots/DetailSection";
import { MemberPicker } from "@/bots/MemberPicker";
import { useBotsStore } from "@/bots/bots-store";
import {
  AlertDialog,
  AlertDialogAction,
  AlertDialogCancel,
  AlertDialogContent,
  AlertDialogDescription,
  AlertDialogFooter,
  AlertDialogHeader,
  AlertDialogTitle,
} from "@/components/ui/alert-dialog";
import { Button } from "@/components/ui/button";
import { errorMessage } from "@/lib/error-message";
import { copy } from "@/ui-copy";

/** The details tab while a channel is picked: its name, the bots in it, and the members it can change. */
export function ChannelDetails({ channel }: { channel: ChannelSummary }) {
  const bots = useBotsStore((state) => state.bots) ?? [];
  const updateMembers = useBotsStore((state) => state.updateMembers);
  const removeChannel = useBotsStore((state) => state.removeChannel);
  const [editing, setEditing] = useState(false);
  const [draft, setDraft] = useState<string[]>([]);
  const [saving, setSaving] = useState(false);
  const [asking, setAsking] = useState(false);
  // A channel that is answering cannot be deleted, so the panel says so instead of failing.
  const answering = useBotChatStore(
    (state) => threadOf(state, channelThread(channel.id)).thinking,
  );
  const text = copy.bots.channels;
  const details = copy.bots.details;
  const members = bots.filter((bot) => channel.members.includes(bot.id));

  async function save() {
    if (saving) return;
    setSaving(true);
    try {
      await updateMembers(channel.id, draft);
      setEditing(false);
    } catch (error) {
      toast.error(text.saveMembersFailed, { description: errorMessage(error) });
    } finally {
      setSaving(false);
    }
  }

  async function remove() {
    try {
      await removeChannel(channel.id);
    } catch (error) {
      toast.error(text.deleteFailed, { description: errorMessage(error) });
    }
  }

  return (
    <div className="flex h-full min-h-0 flex-col gap-4 overflow-y-auto p-4">
      <div className="flex items-center gap-3">
        <span
          aria-hidden="true"
          className="grid size-10 shrink-0 place-items-center rounded-lg bg-muted text-lg text-muted-foreground"
        >
          #
        </span>
        <div className="min-w-0">
          <p className="truncate text-ui font-medium">{channel.name}</p>
          <p className="truncate text-meta text-muted-foreground">
            {text.memberCount(members.length)}
          </p>
        </div>
      </div>

      <DetailSection
        title={text.members}
        actions={
          !editing && (
            <Button
              variant="ghost"
              size="xs"
              aria-label={text.editMembers}
              onClick={() => {
                setDraft(channel.members);
                setEditing(true);
              }}
            >
              {details.edit}
            </Button>
          )
        }
      >
        {editing ? (
          <div className="grid gap-2">
            <MemberPicker bots={bots} value={draft} onChange={setDraft} />
            <div className="flex justify-end gap-2">
              <Button
                variant="outline"
                size="sm"
                onClick={() => setEditing(false)}
              >
                {details.cancel}
              </Button>
              <Button size="sm" disabled={saving} onClick={() => void save()}>
                {details.save}
              </Button>
            </div>
          </div>
        ) : members.length === 0 ? (
          <p className="text-ui text-muted-foreground">{text.noMembers}</p>
        ) : (
          <ul
            role="list"
            aria-label={text.members}
            className="grid grid-cols-1 gap-1"
          >
            {members.map((bot) => (
              <DetailRow
                key={bot.id}
                title={bot.name}
                subtitle={bot.team ?? undefined}
                leading={<BotAvatar name={bot.name} color={bot.color} />}
              />
            ))}
          </ul>
        )}
      </DetailSection>

      <div className="mt-auto grid gap-2 border-t border-border pt-4">
        {answering && (
          <p className="text-meta text-muted-foreground">{text.deleteBusy}</p>
        )}
        <Button
          variant="destructive"
          size="sm"
          disabled={answering}
          onClick={() => setAsking(true)}
        >
          {text.delete}
        </Button>
      </div>

      <AlertDialog open={asking} onOpenChange={setAsking}>
        <AlertDialogContent>
          <AlertDialogHeader>
            <AlertDialogTitle>
              {text.deleteTitle(channel.name)}
            </AlertDialogTitle>
            <AlertDialogDescription>{text.deleteBody}</AlertDialogDescription>
          </AlertDialogHeader>
          <AlertDialogFooter>
            <AlertDialogCancel>{details.cancel}</AlertDialogCancel>
            <AlertDialogAction
              variant="destructive"
              onClick={() => void remove()}
            >
              {text.deleteConfirm}
            </AlertDialogAction>
          </AlertDialogFooter>
        </AlertDialogContent>
      </AlertDialog>
    </div>
  );
}
