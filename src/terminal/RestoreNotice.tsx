import { useState } from "react";

import { Button } from "@/components/ui/button";
import { restoreSession } from "@/terminal/terminal-actions";
import { copy } from "@/ui-copy";
import { cn } from "@/lib/utils";

/** The screen of a session that was open when the app quit: it has no process until the person restores it. */
export function RestoreNotice({
  terminalId,
  className,
}: {
  terminalId: string;
  className?: string;
}) {
  const [restoring, setRestoring] = useState(false);

  const restore = () => {
    setRestoring(true);
    void restoreSession(terminalId).finally(() => setRestoring(false));
  };

  return (
    <div
      className={cn(
        "flex size-full flex-col items-start justify-center gap-2 bg-background p-6",
        className,
      )}
    >
      <p className="text-ui text-foreground">{copy.terminal.restore.notice}</p>
      <p className="text-meta text-muted-foreground">
        {copy.terminal.restore.hint}
      </p>
      <Button size="sm" disabled={restoring} onClick={restore}>
        {copy.terminal.restore.button}
      </Button>
    </div>
  );
}
