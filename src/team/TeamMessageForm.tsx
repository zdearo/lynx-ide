import { useId, useState, type FormEvent } from "react";

import { Button } from "@/components/ui/button";
import { Input } from "@/components/ui/input";
import { Label } from "@/components/ui/label";
import {
  Select,
  SelectContent,
  SelectItem,
  SelectTrigger,
  SelectValue,
} from "@/components/ui/select";
import type { TeamMember } from "@/team/team-fixtures";
import type { TeamPair } from "@/team/team-pairs";
import {
  canStartConversation,
  isSendable,
  MESSAGE_MAX_LENGTH,
  messageRecipients,
  sendTeamMessage,
  showsMessageForm,
} from "@/team/team-message";
import { useTerminalStore } from "@/terminal/terminal-store";
import { copy } from "@/ui-copy";

/**
 * A member's message form in its sheet. It writes into a paired member's terminal. A member with no orchestrator
 * above it, and no orchestrator of its own, only gets the line that says it answers.
 */
export function TeamMessageForm({
  member,
  members,
  pairs,
}: {
  member: TeamMember;
  members: readonly TeamMember[];
  pairs: readonly TeamPair[];
}) {
  const sessions = useTerminalStore((state) => state.sessions);
  const recipients = messageRecipients(member, members, pairs, sessions);
  const [picked, setPicked] = useState<string | null>(null);
  const [draft, setDraft] = useState("");
  const [sent, setSent] = useState<string | null>(null);
  const [sending, setSending] = useState(false);
  const fieldId = useId();

  if (!canStartConversation(member)) {
    return (
      <p className="text-nested text-muted-foreground">
        {copy.team.message.respondsOnly}
      </p>
    );
  }
  if (!showsMessageForm(member, sessions)) return null;

  const target =
    recipients.find((recipient) => recipient.id === picked) ?? recipients[0];
  const tooLong = draft.length > MESSAGE_MAX_LENGTH;
  // An empty box shows no hint: its disabled Send already says so. Spaces only are named, since they look typed.
  const hint = tooLong
    ? copy.team.message.tooLong(MESSAGE_MAX_LENGTH)
    : draft.length > 0 && draft.trim() === ""
      ? copy.team.message.empty
      : null;

  async function submit(event: FormEvent) {
    event.preventDefault();
    if (!target || sending || !isSendable(draft)) return;
    setSending(true);
    try {
      // A refused send keeps the text, so it can be sent again.
      if (await sendTeamMessage(member.role, target.sessionId, draft)) {
        setDraft("");
        setSent(copy.team.message.sent(target.role));
      }
    } finally {
      setSending(false);
    }
  }

  return (
    <section className="grid gap-1.5">
      <h4 className="text-nested text-muted-foreground">
        {copy.team.message.label}
      </h4>
      <form
        onSubmit={submit}
        className="grid gap-1.5"
        onKeyDown={(event) => {
          // Escape is for the text, not for clearing the picked member.
          if (event.key === "Escape") event.stopPropagation();
        }}
      >
        {target ? (
          <>
            <div className="flex items-center gap-1.5">
              <Label
                htmlFor={`${fieldId}-to`}
                className="shrink-0 text-muted-foreground"
              >
                {copy.team.message.to}
              </Label>
              <Select value={target.id} onValueChange={setPicked}>
                <SelectTrigger
                  id={`${fieldId}-to`}
                  size="sm"
                  className="min-w-0 flex-1"
                >
                  <SelectValue />
                </SelectTrigger>
                <SelectContent>
                  {recipients.map((recipient) => (
                    <SelectItem key={recipient.id} value={recipient.id}>
                      {recipient.role}
                    </SelectItem>
                  ))}
                </SelectContent>
              </Select>
            </div>
            <div className="flex items-center gap-1.5">
              <Input
                id={`${fieldId}-text`}
                aria-label={copy.team.message.label}
                placeholder={copy.team.message.placeholder}
                value={draft}
                aria-invalid={tooLong || undefined}
                aria-describedby={hint ? `${fieldId}-hint` : undefined}
                className="min-w-0 flex-1"
                onChange={(event) => {
                  setDraft(event.target.value);
                  setSent(null);
                }}
              />
              <Button
                type="submit"
                size="sm"
                disabled={!isSendable(draft) || sending}
              >
                {copy.team.message.send}
              </Button>
            </div>
            {hint && (
              <p
                id={`${fieldId}-hint`}
                className="text-nested text-muted-foreground"
              >
                {hint}
              </p>
            )}
          </>
        ) : (
          <p className="text-nested text-muted-foreground">
            {copy.team.message.noRecipient}
          </p>
        )}
        <p aria-live="polite" className="text-meta text-muted-foreground">
          {sent}
        </p>
      </form>
    </section>
  );
}
