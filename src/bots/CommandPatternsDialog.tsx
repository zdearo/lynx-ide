import { useState } from "react";

import { commands } from "@/bindings";
import { Button } from "@/components/ui/button";
import {
  Dialog,
  DialogContent,
  DialogDescription,
  DialogFooter,
  DialogHeader,
  DialogTitle,
} from "@/components/ui/dialog";
import { Textarea } from "@/components/ui/textarea";
import { errorMessage } from "@/lib/error-message";
import { unwrap } from "@/lib/ipc-result";
import { copy } from "@/ui-copy";

/** Adds command patterns to a bot, one per line. Lynx refuses any that would allow everything. */
export function CommandPatternsDialog({
  botId,
  onAdded,
  onClose,
}: {
  botId: string;
  onAdded: (patterns: string[]) => void;
  onClose: () => void;
}) {
  const [draft, setDraft] = useState("");
  const [saving, setSaving] = useState(false);
  const [problem, setProblem] = useState<string | null>(null);
  const text = copy.bots.commands.dialog;
  const patterns = draft
    .split("\n")
    .map((line) => line.trim())
    .filter(Boolean);

  async function add() {
    if (patterns.length === 0 || saving) return;
    setSaving(true);
    setProblem(null);
    try {
      onAdded(await unwrap(commands.botsAllowCommands(botId, patterns)));
      onClose();
    } catch (error) {
      setProblem(errorMessage(error));
    } finally {
      setSaving(false);
    }
  }

  return (
    <Dialog open onOpenChange={(open) => !open && onClose()}>
      <DialogContent>
        <DialogHeader>
          <DialogTitle>{text.title}</DialogTitle>
          <DialogDescription>{text.body}</DialogDescription>
        </DialogHeader>
        <Textarea
          value={draft}
          rows={6}
          aria-label={text.label}
          placeholder={text.placeholder}
          className="font-mono text-meta"
          autoFocus
          onChange={(event) => setDraft(event.target.value)}
        />
        {problem && (
          <p role="alert" className="text-meta text-destructive">
            {problem}
          </p>
        )}
        <DialogFooter>
          <Button variant="outline" onClick={onClose}>
            {text.cancel}
          </Button>
          <Button
            disabled={patterns.length === 0 || saving}
            onClick={() => void add()}
          >
            {text.add}
          </Button>
        </DialogFooter>
      </DialogContent>
    </Dialog>
  );
}
