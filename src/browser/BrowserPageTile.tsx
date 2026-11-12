import type { BrowserPageInfo } from "@/bindings";
import { useBrowserStore } from "@/browser/browser-store";
import { agentName, isHarnessId } from "@/browser/browser-tabs";
import { AgentBrandIcon } from "@/components/icons/brands/AgentBrandIcon";
import { XIcon } from "@/components/icons/line-icons";
import { TooltipIconButton } from "@/components/TooltipIconButton";
import { cn } from "@/lib/utils";
import { copy } from "@/ui-copy";

interface BrowserPageTileProps {
  page: BrowserPageInfo;
  /** The last picture of the page, as base64 PNG, when there is one. */
  thumbnail: string | undefined;
  /** This is the tab the person is looking at. */
  shown: boolean;
}

/** One tab in the Browser panel: its picture, title, address and what it is doing. */
export function BrowserPageTile({
  page,
  thumbnail,
  shown,
}: BrowserPageTileProps) {
  const showPage = useBrowserStore((state) => state.showPage);
  const closePage = useBrowserStore((state) => state.closePage);
  const title = page.title || copy.browser.panel.untitled;

  // The close button is a sibling of the show button: a button cannot hold another one.
  return (
    <li className="group relative min-w-0">
      <button
        type="button"
        aria-current={shown ? "true" : undefined}
        onClick={() => void showPage(page.id)}
        className={cn(
          "grid w-full min-w-0 gap-1.5 rounded-lg p-1.5 text-left outline-none focus-visible:ring-[3px] focus-visible:ring-ring/50",
          shown ? "bg-sidebar-accent" : "hover:bg-sidebar-accent/50",
        )}
      >
        <span className="block aspect-[16/10] w-full overflow-hidden rounded-md border border-border bg-muted">
          {thumbnail && (
            <img
              src={`data:image/png;base64,${thumbnail}`}
              alt=""
              className="size-full object-cover object-top"
            />
          )}
        </span>
        <span className="grid min-w-0 gap-0.5 px-0.5">
          <span className="block truncate text-ui font-medium">{title}</span>
          <span className="block truncate font-mono text-meta text-muted-foreground">
            {page.url}
          </span>
          {page.agent !== null ? (
            <span className="inline-flex max-w-full min-w-0 items-center gap-1.5 self-start rounded-full bg-agent/15 px-2 py-0.5 text-meta whitespace-nowrap text-foreground">
              {isHarnessId(page.agent) && (
                <AgentBrandIcon
                  harness={page.agent}
                  className="size-3 shrink-0"
                />
              )}
              <span
                aria-hidden="true"
                className="size-1.5 shrink-0 animate-pulse rounded-full bg-agent motion-reduce:animate-none"
              />
              <span className="min-w-0 truncate">
                {copy.browser.panel.working(agentName(page.agent))}
              </span>
            </span>
          ) : (
            <span className="flex min-w-0 items-center gap-1.5 text-meta text-muted-foreground">
              <span
                aria-hidden="true"
                className={cn(
                  "size-1.5 shrink-0 rounded-full",
                  page.loading ? "bg-live" : "ring-1 ring-muted-foreground",
                )}
              />
              <span className="truncate">
                {page.loading
                  ? copy.browser.panel.loading
                  : copy.browser.panel.loaded}
              </span>
            </span>
          )}
          {page.detached && (
            <span className="block truncate text-meta text-muted-foreground">
              {copy.browser.panel.inWindow}
            </span>
          )}
          {page.agentPage && (
            <span className="block truncate text-meta text-muted-foreground">
              {copy.browser.panel.agentTab}
            </span>
          )}
        </span>
      </button>
      <TooltipIconButton
        label={copy.browser.panel.close(title)}
        onClick={() => void closePage(page.id)}
        className="absolute top-2.5 right-2.5 bg-sidebar/80 text-muted-foreground opacity-0 transition-opacity group-focus-within:opacity-100 group-hover:opacity-100 hover:text-foreground"
      >
        <XIcon className="size-3" />
      </TooltipIconButton>
    </li>
  );
}
