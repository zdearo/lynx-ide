import type { ReactNode } from "react";

import { languageOf, textOf } from "@/bots/code-text";
import { CopyButton } from "@/bots/CopyButton";
import { copy } from "@/ui-copy";

/** A fenced block of code: its language on the left, and a button that copies it on the right. */
export function CodeBlock({ children }: { children?: ReactNode }) {
  const language = languageOf(children);
  // A fence ends its last line with a line break that is not part of the code.
  const code = textOf(children).replace(/\n$/, "");
  return (
    <div className="group my-2 rounded-md bg-muted">
      <div className="flex min-h-6 items-center justify-between gap-2 pt-1 pr-1 pl-3">
        <span className="font-mono text-meta text-muted-foreground">
          {language}
        </span>
        <CopyButton text={code} label={copy.bots.chat.copyCode} />
      </div>
      <pre className="overflow-x-auto px-3 pt-1 pb-3 font-mono text-meta">
        {children}
      </pre>
    </div>
  );
}
