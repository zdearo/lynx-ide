import { useBrowserStore } from "@/browser/browser-store";
import { worktreePages } from "@/browser/browser-tabs";
import { BrowserPageTile } from "@/browser/BrowserPageTile";
import { PlusGlyph18 } from "@/components/icons/PlusGlyph18";
import { Button } from "@/components/ui/button";
import { ScrollArea } from "@/components/ui/scroll-area";
import { copy } from "@/ui-copy";
import { useSelectedWorktree } from "@/workspace/worktrees/use-selected-worktree";

/** The browser tabs of the selected worktree, each with a picture of its page. */
export function BrowserTabPanel() {
  const worktreePath = useSelectedWorktree()?.worktree.path ?? null;
  const pages = useBrowserStore((state) => state.pages);
  const thumbnails = useBrowserStore((state) => state.thumbnails);
  const shownPage = useBrowserStore((state) => state.state.page);
  const newPage = useBrowserStore((state) => state.newPage);
  const tabs = worktreePath ? worktreePages(pages, worktreePath) : [];

  const createTab = () => {
    if (worktreePath) void newPage(worktreePath, null);
  };

  return (
    <div className="flex h-full min-h-0 flex-col">
      <div className="flex shrink-0 items-center gap-2 py-1.5 pr-2 pl-3">
        <h2 className="text-ui font-semibold">{copy.browser.panel.title}</h2>
        {tabs.length > 0 && (
          <span className="text-meta text-muted-foreground tabular-nums">
            {copy.browser.panel.count(tabs.length)}
          </span>
        )}
        {worktreePath && (
          <Button
            type="button"
            variant="ghost"
            size="sm"
            className="ml-auto h-[26px] gap-1.5 px-2 text-nested text-muted-foreground hover:text-foreground"
            onClick={createTab}
          >
            <PlusGlyph18 className="size-3.5" />
            {copy.browser.panel.newTab}
          </Button>
        )}
      </div>

      {tabs.length > 0 ? (
        <ScrollArea className="min-h-0 flex-1">
          <ul className="grid grid-cols-1 content-start gap-1 px-1.5 pt-1 pb-2.5">
            {tabs.map((page) => (
              <BrowserPageTile
                key={page.id}
                page={page}
                thumbnail={thumbnails[page.id]}
                shown={shownPage === page.id}
              />
            ))}
          </ul>
        </ScrollArea>
      ) : (
        <div className="grid justify-items-center gap-2 px-3.5 py-5 text-center">
          <h3 className="text-ui font-semibold">
            {copy.browser.panel.empty.title}
          </h3>
          <p className="max-w-60 text-nested text-muted-foreground">
            {copy.browser.panel.empty.body}
          </p>
          {worktreePath && (
            <Button
              type="button"
              size="sm"
              className="mt-1"
              onClick={createTab}
            >
              {copy.browser.panel.newTab}
            </Button>
          )}
        </div>
      )}
    </div>
  );
}
