import { useState } from "react";

import { commands, type BotMcpServer } from "@/bindings";
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

/** Adds a bot's MCP servers from a config pasted in the format Claude Code uses. Lynx checks it first. */
export function McpServerDialog({
  botId,
  onAdded,
  onClose,
}: {
  botId: string;
  onAdded: (servers: BotMcpServer[]) => void;
  onClose: () => void;
}) {
  const [config, setConfig] = useState("");
  const [saving, setSaving] = useState(false);
  const [problem, setProblem] = useState<string | null>(null);
  const text = copy.bots.mcp.dialog;

  async function add() {
    if (!config.trim() || saving) return;
    setSaving(true);
    setProblem(null);
    try {
      onAdded(await unwrap(commands.botsMcpAdd(botId, config)));
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
          value={config}
          rows={10}
          aria-label={text.label}
          placeholder={text.placeholder}
          className="font-mono text-meta"
          autoFocus
          onChange={(event) => setConfig(event.target.value)}
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
            disabled={!config.trim() || saving}
            onClick={() => void add()}
          >
            {text.add}
          </Button>
        </DialogFooter>
      </DialogContent>
    </Dialog>
  );
}
