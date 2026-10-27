import Markdown, { type Components } from "react-markdown";
import remarkGfm from "remark-gfm";
import { toast } from "sonner";

import { commands } from "@/bindings";
import { CodeBlock } from "@/bots/CodeBlock";
import { errorMessage } from "@/lib/error-message";
import { unwrap } from "@/lib/ipc-result";
import { copy } from "@/ui-copy";

/** Opens a link in the browser, so the app's own window stays where it is. */
function openLink(url: string) {
  unwrap(commands.openLink(url)).catch((error: unknown) =>
    toast.error(copy.bots.chat.linkFailed, {
      description: errorMessage(error),
    }),
  );
}

// Every element gets our own look. An https link opens in the browser; the app's window never navigates.
const COMPONENTS: Components = {
  p: ({ children }) => (
    <p className="my-1.5 first:mt-0 last:mb-0">{children}</p>
  ),
  ul: ({ children }) => (
    <ul className="my-1.5 list-disc pl-5 marker:text-muted-foreground">
      {children}
    </ul>
  ),
  ol: ({ children }) => (
    <ol className="my-1.5 list-decimal pl-5 marker:text-muted-foreground">
      {children}
    </ol>
  ),
  li: ({ children }) => <li className="my-0.5">{children}</li>,
  h1: ({ children }) => (
    <p className="mt-3 mb-1 font-medium first:mt-0">{children}</p>
  ),
  h2: ({ children }) => (
    <p className="mt-3 mb-1 font-medium first:mt-0">{children}</p>
  ),
  h3: ({ children }) => (
    <p className="mt-3 mb-1 font-medium first:mt-0">{children}</p>
  ),
  blockquote: ({ children }) => (
    <blockquote className="my-1.5 border-l-2 border-border pl-3 text-muted-foreground">
      {children}
    </blockquote>
  ),
  pre: ({ children }) => <CodeBlock>{children}</CodeBlock>,
  code: ({ children }) => (
    <code className="rounded bg-muted px-1 py-0.5 font-mono text-meta [pre_&]:bg-transparent [pre_&]:p-0">
      {children}
    </code>
  ),
  a: ({ children, href }) =>
    href?.startsWith("https://") ? (
      <a
        href={href}
        className="underline underline-offset-2"
        onClick={(event) => {
          event.preventDefault();
          openLink(href);
        }}
      >
        {children}
      </a>
    ) : (
      <span className="underline underline-offset-2" title={href}>
        {children}
      </span>
    ),
  img: ({ alt }) => <span>{alt}</span>,
  table: ({ children }) => (
    <div className="my-2 overflow-x-auto">
      <table className="w-full border-collapse text-meta">{children}</table>
    </div>
  ),
  th: ({ children }) => (
    <th className="border-b border-border px-2 py-1 text-left font-medium">
      {children}
    </th>
  ),
  td: ({ children }) => (
    <td className="border-b border-border px-2 py-1">{children}</td>
  ),
};

/** A bot's words, as Markdown. Raw HTML in them is left out, so nothing in a reply can run. */
export function BotMarkdown({ children }: { children: string }) {
  return (
    <Markdown remarkPlugins={[remarkGfm]} components={COMPONENTS}>
      {children}
    </Markdown>
  );
}
