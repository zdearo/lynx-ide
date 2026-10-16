import { useState, type FormEvent } from "react";

import { Button } from "@/components/ui/button";
import {
  Dialog,
  DialogContent,
  DialogDescription,
  DialogFooter,
  DialogHeader,
  DialogTitle,
} from "@/components/ui/dialog";
import { Input } from "@/components/ui/input";
import { Label } from "@/components/ui/label";
import { useDialogStore } from "@/dialog-store";
import { startTerminal } from "@/terminal/terminal-actions";
import { copy } from "@/ui-copy";

export function CustomCommandDialog() {
  const worktreePath = useDialogStore((state) => state.customCommandFor);
  const askCustomCommand = useDialogStore((state) => state.askCustomCommand);

  return (
    <Dialog
      open={worktreePath !== null}
      onOpenChange={(open) => {
        if (!open) askCustomCommand(null);
      }}
    >
      {worktreePath && (
        <CustomCommandForm
          worktreePath={worktreePath}
          onClose={() => askCustomCommand(null)}
        />
      )}
    </Dialog>
  );
}

function CustomCommandForm({
  worktreePath,
  onClose,
}: {
  worktreePath: string;
  onClose: () => void;
}) {
  const [command, setCommand] = useState("");
  const canRun = command.trim().length > 0;

  function submit(event: FormEvent) {
    event.preventDefault();
    if (!canRun) return;
    void startTerminal(worktreePath, command);
    onClose();
  }

  return (
    <DialogContent className="sm:max-w-md">
      <form onSubmit={submit} className="grid gap-4">
        <DialogHeader>
          <DialogTitle>{copy.customCommand.title}</DialogTitle>
          <DialogDescription>
            {copy.customCommand.description}
          </DialogDescription>
        </DialogHeader>
        <div className="grid gap-1.5">
          <Label htmlFor="custom-command">{copy.customCommand.label}</Label>
          <Input
            id="custom-command"
            value={command}
            autoComplete="off"
            spellCheck={false}
            placeholder={copy.customCommand.placeholder}
            onChange={(event) => setCommand(event.target.value)}
          />
        </div>
        <DialogFooter>
          <Button type="button" variant="ghost" onClick={onClose}>
            {copy.customCommand.cancel}
          </Button>
          <Button type="submit" disabled={!canRun}>
            {copy.customCommand.run}
          </Button>
        </DialogFooter>
      </form>
    </DialogContent>
  );
}
