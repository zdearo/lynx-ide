import { useState, type FormEvent } from "react";
import { toast } from "sonner";

import type { BotSummary } from "@/bindings";
import { MemberPicker } from "@/bots/MemberPicker";
import { useBotsStore } from "@/bots/bots-store";
import { Button } from "@/components/ui/button";
import {
  Dialog,
  DialogContent,
  DialogFooter,
  DialogHeader,
  DialogTitle,
} from "@/components/ui/dialog";
import { Input } from "@/components/ui/input";
import { errorMessage } from "@/lib/error-message";
import { copy } from "@/ui-copy";

/** Makes a channel: its name, and the bots that sit in it. The person is always in it. */
export function ChannelDialog({
  bots,
  onClose,
}: {
  bots: BotSummary[];
  onClose: () => void;
}) {
  const createChannel = useBotsStore((state) => state.createChannel);
  const [name, setName] = useState("");
  const [members, setMembers] = useState<string[]>([]);
  const [saving, setSaving] = useState(false);
  const text = copy.bots.channels;
  const ready = name.trim().length > 0;

  async function submit(event: FormEvent<HTMLFormElement>) {
    event.preventDefault();
    if (!ready || saving) return;
    setSaving(true);
    try {
      await createChannel({ name: name.trim(), members });
      onClose();
    } catch (error) {
      toast.error(text.createFailed, { description: errorMessage(error) });
    } finally {
      setSaving(false);
    }
  }

  return (
    <Dialog open onOpenChange={(open) => !open && onClose()}>
      <DialogContent className="sm:max-w-sm">
        <DialogHeader>
          <DialogTitle>{text.newChannel}</DialogTitle>
        </DialogHeader>
        <form onSubmit={(event) => void submit(event)} className="grid gap-4">
          <label className="grid gap-1.5 text-meta text-muted-foreground">
            {text.name}
            <Input
              value={name}
              maxLength={40}
              placeholder={text.namePlaceholder}
              autoFocus
              onChange={(event) => setName(event.target.value)}
            />
          </label>
          <fieldset className="grid gap-1.5">
            <legend className="text-meta text-muted-foreground">
              {text.members}
            </legend>
            <MemberPicker bots={bots} value={members} onChange={setMembers} />
          </fieldset>
          <DialogFooter>
            <Button type="button" variant="outline" onClick={onClose}>
              {text.cancel}
            </Button>
            <Button type="submit" disabled={!ready || saving}>
              {text.create}
            </Button>
          </DialogFooter>
        </form>
      </DialogContent>
    </Dialog>
  );
}
