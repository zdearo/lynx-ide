import { useEffect, useState } from "react";
import { toast } from "sonner";

import { commands } from "@/bindings";
import { CommandPatternsDialog } from "@/bots/CommandPatternsDialog";
import { DetailRow, DetailSection } from "@/bots/DetailSection";
import { PlusGlyph18 } from "@/components/icons/PlusGlyph18";
import { XmarkGlyph18 } from "@/components/icons/XmarkGlyph18";
import { TooltipIconButton } from "@/components/TooltipIconButton";
import { errorMessage } from "@/lib/error-message";
import { unwrap } from "@/lib/ipc-result";
import { copy } from "@/ui-copy";

/** The command patterns a bot may run, with a way to add some and to take each one back. */
export function BotCommandsSection({ botId }: { botId: string }) {
  const [patterns, setPatterns] = useState<string[] | null>(null);
  const [adding, setAdding] = useState(false);
  const text = copy.bots.commands;

  useEffect(() => {
    let current = true;
    unwrap(commands.botsAllowedCommands(botId))
      .then((found) => {
        if (current) setPatterns(found);
      })
      .catch((error: unknown) =>
        toast.error(text.loadFailed, { description: errorMessage(error) }),
      );
    return () => {
      current = false;
    };
  }, [botId, text.loadFailed]);

  async function remove(pattern: string) {
    try {
      setPatterns(await unwrap(commands.botsDisallowCommand(botId, pattern)));
    } catch (error) {
      toast.error(text.removeFailed, { description: errorMessage(error) });
    }
  }

  return (
    <DetailSection
      title={text.title}
      hint={text.hint}
      actions={
        <TooltipIconButton label={text.add} onClick={() => setAdding(true)}>
          <PlusGlyph18 />
        </TooltipIconButton>
      }
    >
      {patterns !== null &&
        (patterns.length === 0 ? (
          <p className="text-ui text-muted-foreground">{text.none}</p>
        ) : (
          <ul
            role="list"
            aria-label={text.title}
            className="grid grid-cols-1 gap-1"
          >
            {patterns.map((pattern) => (
              <DetailRow
                key={pattern}
                title={pattern}
                mono
                action={
                  <TooltipIconButton
                    label={text.removeLabel(pattern)}
                    onClick={() => void remove(pattern)}
                  >
                    <XmarkGlyph18 />
                  </TooltipIconButton>
                }
              />
            ))}
          </ul>
        ))}
      {adding && (
        <CommandPatternsDialog
          botId={botId}
          onAdded={setPatterns}
          onClose={() => setAdding(false)}
        />
      )}
    </DetailSection>
  );
}
