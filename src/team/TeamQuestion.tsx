import { useState, type FormEvent } from "react";

import { Button } from "@/components/ui/button";
import { Input } from "@/components/ui/input";
import { useTeamStore } from "@/team/team-store";
import { copy } from "@/ui-copy";

/** A member's question to the user, answered inline. The reply sends the member back to work. */
export function TeamQuestion({
  id,
  role,
  question,
}: {
  id: string;
  role: string;
  question: string;
}) {
  const [replying, setReplying] = useState(false);
  const [draft, setDraft] = useState("");

  function send(event: FormEvent<HTMLFormElement>) {
    event.preventDefault();
    const text = draft.trim();
    if (!text) return;
    useTeamStore.getState().reply(id, text);
    setDraft("");
    setReplying(false);
    // The form leaves with the question; keyboard focus stays in the panel that holds it.
    event.currentTarget.closest<HTMLElement>('[role="dialog"]')?.focus();
  }

  return (
    <div className="flex h-8 min-w-0 items-center gap-2">
      {replying ? (
        <form
          onSubmit={send}
          className="flex min-w-0 flex-1 items-center gap-1.5"
        >
          <Input
            autoFocus
            aria-label={copy.team.replyTo(role)}
            placeholder={copy.team.replyPlaceholder}
            value={draft}
            onChange={(event) => setDraft(event.target.value)}
            className="h-7 text-ui"
          />
          <Button type="submit" size="sm">
            {copy.team.send}
          </Button>
        </form>
      ) : (
        <>
          <p className="min-w-0 flex-1 truncate text-nested text-attention">
            {question}
          </p>
          <Button
            variant="outline"
            size="sm"
            className="shrink-0"
            onClick={() => setReplying(true)}
          >
            {copy.team.reply}
          </Button>
        </>
      )}
    </div>
  );
}
