import { useEffect, useState } from "react";
import { toast } from "sonner";

import { CheckGlyph18 } from "@/components/icons/CheckGlyph18";
import { CopyGlyph18 } from "@/components/icons/CopyGlyph18";
import { TooltipIconButton } from "@/components/TooltipIconButton";
import { cn } from "@/lib/utils";
import { copy } from "@/ui-copy";

/** How long the button shows that it copied. */
const COPIED_MS = 1500;

/**
 * Copies `text` to the clipboard. It shows on hover and on keyboard focus of the group around it
 * (the parent needs `group`), and stays while it says "Copied", which a screen reader hears too.
 */
export function CopyButton({
  text,
  label,
  className,
}: {
  text: string;
  label: string;
  className?: string;
}) {
  const [copied, setCopied] = useState(false);
  const words = copy.bots.chat;

  useEffect(() => {
    if (!copied) return;
    const timer = setTimeout(() => setCopied(false), COPIED_MS);
    return () => clearTimeout(timer);
  }, [copied]);

  async function copyIt() {
    try {
      await navigator.clipboard.writeText(text);
      setCopied(true);
    } catch {
      // The clipboard can refuse; the text is still on screen, so it can be copied by hand.
      toast.error(words.copyFailed);
    }
  }

  return (
    <>
      <TooltipIconButton
        label={copied ? words.copied : label}
        data-copied={copied || undefined}
        className={cn(
          "text-muted-foreground opacity-0 transition-opacity group-hover:opacity-100 group-has-[:focus-visible]:opacity-100 focus-visible:opacity-100 data-copied:opacity-100",
          className,
        )}
        onClick={() => void copyIt()}
      >
        {copied ? <CheckGlyph18 /> : <CopyGlyph18 />}
      </TooltipIconButton>
      {/* A live region without the status role: the chat reads status elements as "still working". */}
      <span aria-live="polite" className="sr-only">
        {copied ? words.copied : ""}
      </span>
    </>
  );
}
