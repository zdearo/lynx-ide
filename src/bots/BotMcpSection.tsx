import { useEffect, useState } from "react";
import { PlusIcon, XIcon } from "@phosphor-icons/react";
import { toast } from "sonner";

import { commands, type BotMcpServer } from "@/bindings";
import { DetailRow, DetailSection } from "@/bots/DetailSection";
import { McpServerDialog } from "@/bots/McpServerDialog";
import { TooltipIconButton } from "@/components/TooltipIconButton";
import { errorMessage } from "@/lib/error-message";
import { unwrap } from "@/lib/ipc-result";
import { copy } from "@/ui-copy";

/** The MCP servers a bot can call, with a way to add more from a config and to take each one back. */
export function BotMcpSection({ botId }: { botId: string }) {
  const [servers, setServers] = useState<BotMcpServer[] | null>(null);
  const [adding, setAdding] = useState(false);
  const text = copy.bots.mcp;

  useEffect(() => {
    let current = true;
    unwrap(commands.botsMcpServers(botId))
      .then((found) => {
        if (current) setServers(found);
      })
      .catch((error: unknown) =>
        toast.error(text.loadFailed, { description: errorMessage(error) }),
      );
    return () => {
      current = false;
    };
  }, [botId, text.loadFailed]);

  async function remove(name: string) {
    try {
      setServers(await unwrap(commands.botsMcpRemove(botId, name)));
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
          <PlusIcon />
        </TooltipIconButton>
      }
    >
      {servers !== null &&
        (servers.length === 0 ? (
          <p className="text-ui text-muted-foreground">{text.none}</p>
        ) : (
          <ul
            role="list"
            aria-label={text.title}
            className="grid grid-cols-1 gap-1"
          >
            {servers.map((server) => (
              <DetailRow
                key={server.name}
                title={server.name}
                subtitle={`${text.kind[server.kind]} ${server.target}`}
                mono
                action={
                  <TooltipIconButton
                    label={text.removeLabel(server.name)}
                    onClick={() => void remove(server.name)}
                  >
                    <XIcon />
                  </TooltipIconButton>
                }
              />
            ))}
          </ul>
        ))}
      {adding && (
        <McpServerDialog
          botId={botId}
          onAdded={setServers}
          onClose={() => setAdding(false)}
        />
      )}
    </DetailSection>
  );
}
