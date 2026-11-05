import { useEffect, useState } from "react";

import { commands, type DeleteImpact } from "@/bindings";
import { deleteImpactLines } from "@/bots/bot-delete-impact";
import { unwrap } from "@/lib/ipc-result";
import { copy } from "@/ui-copy";

/**
 * What deleting the bot takes with it, read when the confirm dialog opens (the dialog mounts this only while open).
 * Nothing shows while it reads, or when nothing else goes.
 */
export function BotDeleteImpact({ botId }: { botId: string }) {
  const text = copy.bots.details.impact;
  const [impact, setImpact] = useState<DeleteImpact | null>(null);
  const [failed, setFailed] = useState(false);

  useEffect(() => {
    let current = true;
    unwrap(commands.botsDeleteImpact(botId))
      .then((found) => {
        if (current) setImpact(found);
      })
      .catch(() => {
        if (current) setFailed(true);
      });
    return () => {
      current = false;
    };
  }, [botId]);

  if (failed) {
    return <p className="text-meta text-muted-foreground">{text.failed}</p>;
  }
  if (impact === null) return null;
  const lines = deleteImpactLines(impact, text);
  if (lines.members.length === 0 && lines.notes.length === 0) return null;

  return (
    <div className="grid gap-2 text-meta text-muted-foreground">
      {lines.members.length > 0 && (
        <div>
          <p>{text.members}</p>
          <ul className="list-disc pl-4">
            {lines.members.map((line, index) => (
              <li key={index}>{line}</li>
            ))}
          </ul>
        </div>
      )}
      {lines.notes.map((note) => (
        <p key={note}>{note}</p>
      ))}
    </div>
  );
}
