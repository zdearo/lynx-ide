import { useEffect, useState, type KeyboardEvent } from "react";
import { CaretMinimizeGlyph18 } from "@/components/icons/CaretMinimizeGlyph18";
import { RefreshGlyph18 } from "@/components/icons/RefreshGlyph18";

import { TooltipIconButton } from "@/components/TooltipIconButton";
import { Button } from "@/components/ui/button";
import { ScrollArea } from "@/components/ui/scroll-area";
import { useExplorerStore } from "@/files/explorer-store";
import { FileRow } from "@/files/FileRow";
import { visibleRows, type EntryRow } from "@/files/file-tree-rows";
import { cn } from "@/lib/utils";
import { copy } from "@/ui-copy";
import { useSelectedWorktree } from "@/workspace/use-selected-worktree";
import { worktreeLabel } from "@/workspace/worktree-label";

/** Moves keyboard focus to a row once React has drawn it. */
function focusRow(container: HTMLElement | null, path: string) {
  requestAnimationFrame(() => {
    const rows = container?.querySelectorAll<HTMLElement>("[data-tree-path]");
    rows?.forEach((row) => {
      if (row.dataset.treePath === path) row.focus();
    });
  });
}

/** The files of the selected worktree, as a tree that loads folders when they are opened. */
export function FilesPanel() {
  const selectedWorktree = useSelectedWorktree();
  const worktreePath = selectedWorktree?.worktree.path ?? null;
  const tree = useExplorerStore((state) =>
    worktreePath ? state.trees[worktreePath] : undefined,
  );
  const [menuFor, setMenuFor] = useState<string | null>(null);
  const [treeElement, setTreeElement] = useState<HTMLDivElement | null>(null);

  useEffect(() => {
    if (worktreePath) void useExplorerStore.getState().load(worktreePath, "");
  }, [worktreePath]);

  if (!selectedWorktree || !worktreePath) {
    return (
      <p className="px-3 pt-2 text-ui text-muted-foreground">
        {copy.files.noWorktree}
      </p>
    );
  }

  const path = worktreePath;
  const store = useExplorerStore.getState();
  const rows = tree ? visibleRows(tree) : [];
  const entries = rows.filter((row): row is EntryRow => row.type === "entry");
  const root = tree?.directories[""];
  const selectedPath = tree?.selected ?? null;
  // Exactly one row takes the Tab key: the selected one, else the first.
  const tabStop = entries.some((row) => row.path === selectedPath)
    ? selectedPath
    : (entries[0]?.path ?? null);

  const activate = (row: EntryRow) => {
    store.select(path, row.path);
    if (row.isDirectory) void store.toggle(path, row.path);
  };

  const moveTo = (row: EntryRow | undefined) => {
    if (!row) return;
    store.select(path, row.path);
    focusRow(treeElement, row.path);
  };

  function onKeyDown(event: KeyboardEvent<HTMLDivElement>) {
    const index = entries.findIndex((row) => row.path === selectedPath);
    const current = entries[index];
    if (!current) return;

    switch (event.key) {
      case "ArrowDown":
        moveTo(entries[Math.min(entries.length - 1, index + 1)]);
        break;
      case "ArrowUp":
        moveTo(entries[Math.max(0, index - 1)]);
        break;
      case "Home":
        moveTo(entries[0]);
        break;
      case "End":
        moveTo(entries.at(-1));
        break;
      case "ArrowRight":
        if (current.isDirectory && !current.expanded) {
          void store.toggle(path, current.path);
        } else if (current.isDirectory) {
          moveTo(
            entries[index + 1]?.depth === current.depth + 1
              ? entries[index + 1]
              : undefined,
          );
        }
        break;
      case "ArrowLeft":
        if (current.isDirectory && current.expanded) {
          void store.toggle(path, current.path);
        } else {
          // Up to the folder that holds this row.
          const parent = [...entries.slice(0, index)]
            .reverse()
            .find((row) => row.depth === current.depth - 1);
          moveTo(parent);
        }
        break;
      case "Enter":
      case " ":
        if (current.isDirectory) void store.toggle(path, current.path);
        break;
      case "ContextMenu":
        setMenuFor(current.path);
        break;
      case "F10":
        if (event.shiftKey) setMenuFor(current.path);
        else return;
        break;
      default:
        return;
    }
    event.preventDefault();
  }

  const loadingFirstTime =
    !root || (root.status === "loading" && root.entries.length === 0);

  return (
    <div className="flex h-full flex-col">
      <div className="flex h-8 shrink-0 items-center justify-between pr-1.5 pl-3">
        <h2 className="min-w-0 truncate text-xs font-medium text-muted-foreground">
          {copy.files.title}
          <span className="font-normal">
            {" "}
            · {worktreeLabel(selectedWorktree.worktree).text}
          </span>
        </h2>
        <div className="flex items-center">
          <TooltipIconButton
            label={copy.files.refresh}
            onClick={() => void store.refresh(path)}
          >
            <RefreshGlyph18
              className={cn(root?.status === "loading" && "animate-spin")}
            />
          </TooltipIconButton>
          <TooltipIconButton
            label={copy.files.collapseAll}
            onClick={() => store.collapseAll(path)}
          >
            <CaretMinimizeGlyph18 />
          </TooltipIconButton>
        </div>
      </div>

      {root?.status === "error" && root.entries.length === 0 ? (
        <div className="px-3 pt-1" role="alert">
          <p className="text-ui text-destructive">{copy.files.loadFailed}</p>
          <p className="mt-1 text-meta break-words text-muted-foreground">
            {root.error}
          </p>
          <Button
            variant="outline"
            size="sm"
            className="mt-2"
            onClick={() => void store.load(path, "")}
          >
            {copy.files.retry}
          </Button>
        </div>
      ) : loadingFirstTime ? (
        <p className="px-3 pt-1 text-ui text-muted-foreground" aria-busy="true">
          {copy.files.loading}
        </p>
      ) : (
        <ScrollArea className="min-h-0 flex-1">
          <div
            ref={setTreeElement}
            role="tree"
            aria-label={copy.files.tree}
            onKeyDown={onKeyDown}
            className="px-1.5 pb-2"
          >
            {rows.map((row) =>
              row.type === "entry" ? (
                <FileRow
                  key={row.path}
                  row={row}
                  worktreePath={path}
                  selected={row.path === selectedPath}
                  focusable={row.path === tabStop}
                  menuOpen={menuFor === row.path}
                  onMenuOpenChange={(open) =>
                    setMenuFor(open ? row.path : null)
                  }
                  onActivate={() => activate(row)}
                />
              ) : (
                <p
                  key={row.path}
                  role="none"
                  style={{ paddingLeft: 6 + row.depth * 12 + 20 }}
                  className={cn(
                    "h-6 truncate pr-2 text-meta leading-6",
                    row.kind === "error"
                      ? "text-destructive"
                      : "text-muted-foreground",
                  )}
                >
                  {row.text}
                </p>
              ),
            )}
          </div>
        </ScrollArea>
      )}
    </div>
  );
}
