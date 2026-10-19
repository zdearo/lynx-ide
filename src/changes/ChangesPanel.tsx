import type { KeyboardEvent } from "react";

import { ChevronRightGlyph18 } from "@/components/icons/ChevronRightGlyph18";
import { PlusGlyph18 } from "@/components/icons/PlusGlyph18";
import { RefreshGlyph18 } from "@/components/icons/RefreshGlyph18";
import { TooltipIconButton } from "@/components/TooltipIconButton";
import { Button } from "@/components/ui/button";
import { ScrollArea } from "@/components/ui/scroll-area";
import {
  changeSections,
  type ChangeEntry,
  type ChangeSectionId,
} from "@/changes/change-sections";
import { useCollapsedChangeSections } from "@/changes/change-section-collapse";
import { ChangeRow, type IndexAction } from "@/changes/ChangeRow";
import { CommitArea } from "@/changes/CommitArea";
import { CommitList } from "@/changes/CommitList";
import { useChangesViewStore } from "@/changes/changes-view-store";
import { stageFile, stageFiles, unstageFile } from "@/changes/change-actions";
import { useChangesRefresh } from "@/changes/use-changes-refresh";
import { useChangesStore } from "@/changes/changes-store";
import { useEditorStore } from "@/editor/editor-store";
import { cn } from "@/lib/utils";
import { copy } from "@/ui-copy";
import { useSelectedWorktree } from "@/workspace/use-selected-worktree";
import { worktreeLabel } from "@/workspace/worktree-label";

/** Sections whose files can all be staged at once. Conflicts are resolved by hand, not staged. */
const STAGE_ALL: ReadonlySet<ChangeSectionId> = new Set([
  "changes",
  "untracked",
]);

/** Conflicts and staged files stay open: they are what needs a look before a commit. */
const FOLDABLE: ReadonlySet<ChangeSectionId> = new Set([
  "changes",
  "untracked",
]);

const rowKey = (entry: ChangeEntry, section: string) =>
  `${section}:${entry.file.path}`;

/** The files that changed in the selected worktree, grouped as Git groups them. */
export function ChangesPanel() {
  const selected = useSelectedWorktree();
  const worktreePath = selected?.worktree.path ?? null;
  const changes = useChangesStore((state) =>
    worktreePath ? state.byWorktree[worktreePath] : undefined,
  );
  const activePath = useEditorStore((state) =>
    worktreePath ? state.active[worktreePath] : null,
  );
  const view = useChangesViewStore((state) => state.view);
  const setView = useChangesViewStore((state) => state.setView);
  const collapsed = useCollapsedChangeSections((state) => state.collapsed);
  const toggleSection = useCollapsedChangeSections((state) => state.toggle);

  useChangesRefresh(worktreePath);

  if (!selected || !worktreePath) {
    return (
      <p className="px-3 pt-2 text-ui text-muted-foreground">
        {copy.changes.noWorktree}
      </p>
    );
  }

  const path = worktreePath;
  const sections = changeSections(changes?.files ?? []);
  const count = changes?.files.length ?? 0;

  function show(entry: ChangeEntry, preview: boolean) {
    void useEditorStore.getState().open(path, entry.file.path, {
      preview,
      view: "diff",
      originalPath: entry.file.originalPath,
    });
  }

  function onKeyDown(event: KeyboardEvent<HTMLDivElement>) {
    const rows = [
      ...event.currentTarget.querySelectorAll<HTMLElement>(
        "[data-change-path]",
      ),
    ];
    const index = rows.findIndex((row) => row === document.activeElement);
    if (index < 0) return;
    const move = (to: number) => {
      event.preventDefault();
      rows[Math.max(0, Math.min(rows.length - 1, to))]?.focus();
    };
    switch (event.key) {
      case "ArrowDown":
        move(index + 1);
        break;
      case "ArrowUp":
        move(index - 1);
        break;
      case "Home":
        move(0);
        break;
      case "End":
        move(rows.length - 1);
        break;
    }
  }

  const loadingFirstTime = !changes || (changes.status === "loading" && !count);
  const stagedCount = changes?.files.filter((file) => file.staged).length ?? 0;

  // Staged files leave the index from their own section; changes and untracked files go in.
  function indexActionFor(
    section: ChangeSectionId,
    entry: ChangeEntry,
  ): IndexAction | undefined {
    if (section === "staged") {
      return {
        kind: "unstage",
        onRun: () => void unstageFile(path, entry.file),
      };
    }
    if (section === "changes" || section === "untracked") {
      return { kind: "stage", onRun: () => void stageFile(path, entry.file) };
    }
    return undefined;
  }

  return (
    <div className="flex h-full flex-col">
      <div className="flex h-8 shrink-0 items-center justify-between pr-1.5 pl-3">
        <h2 className="min-w-0 truncate text-xs font-medium text-muted-foreground">
          {copy.changes.title}
          <span className="font-normal">
            {" "}
            · {worktreeLabel(selected.worktree).text}
          </span>
        </h2>
        <TooltipIconButton
          label={copy.changes.refresh}
          onClick={() => void useChangesStore.getState().refresh(path)}
        >
          <RefreshGlyph18 />
        </TooltipIconButton>
      </div>

      <CommitArea worktreePath={path} stagedCount={stagedCount} />

      <div
        role="group"
        aria-label={copy.changes.views.label}
        className="flex shrink-0 gap-1 border-b border-sidebar-border px-2 py-1.5"
      >
        {(["changes", "commits"] as const).map((id) => (
          <button
            key={id}
            type="button"
            aria-pressed={view === id}
            onClick={() => setView(id)}
            className={cn(
              "h-6 rounded-md px-2 text-meta font-medium outline-none focus-visible:ring-2 focus-visible:ring-ring/60",
              view === id
                ? "bg-sidebar-accent text-sidebar-accent-foreground"
                : "text-muted-foreground hover:bg-sidebar-accent/50",
            )}
          >
            {copy.changes.views[id]}
            {id === "changes" && count > 0 && (
              <span className="ml-1 font-normal tabular-nums">{count}</span>
            )}
          </button>
        ))}
      </div>

      {view === "commits" ? (
        <CommitList worktreePath={path} />
      ) : changes?.status === "error" && !count ? (
        <div className="px-3 pt-1" role="alert">
          <p className="text-ui text-destructive">{copy.changes.loadFailed}</p>
          <p className="mt-1 text-meta break-words text-muted-foreground">
            {changes.error}
          </p>
          <Button
            variant="outline"
            size="sm"
            className="mt-2"
            onClick={() => void useChangesStore.getState().refresh(path)}
          >
            {copy.changes.retry}
          </Button>
        </div>
      ) : loadingFirstTime ? (
        <p className="px-3 pt-1 text-ui text-muted-foreground" aria-busy="true">
          {copy.changes.loading}
        </p>
      ) : sections.length === 0 ? (
        <p className="px-3 pt-1 text-ui text-muted-foreground">
          {copy.changes.clean}
        </p>
      ) : (
        <ScrollArea className="min-h-0 flex-1">
          <div
            role="group"
            aria-label={copy.changes.list}
            onKeyDown={onKeyDown}
            className="px-1.5 pb-2"
          >
            {sections.map((section, sectionIndex) => {
              const foldable = FOLDABLE.has(section.id);
              const open = !foldable || !collapsed[section.id];
              const listId = `changes-${section.id}-list`;
              const title = (
                <>
                  {copy.changes.sections[section.id]}
                  <span className="font-normal tabular-nums">
                    {section.entries.length}
                  </span>
                </>
              );
              return (
                <div
                  key={section.id}
                  className={cn(sectionIndex > 0 && "mt-2")}
                >
                  <div className="flex h-6 items-center">
                    <h3
                      id={`changes-${section.id}`}
                      className="flex h-6 min-w-0 flex-1 items-center text-meta font-medium tracking-wide text-muted-foreground uppercase"
                    >
                      {foldable ? (
                        <button
                          type="button"
                          aria-expanded={open}
                          aria-controls={open ? listId : undefined}
                          onClick={() => toggleSection(section.id)}
                          className="flex h-full min-w-0 flex-1 items-center gap-1.5 rounded-md pl-1.5 text-left uppercase outline-none hover:text-foreground focus-visible:ring-2 focus-visible:ring-ring/60 focus-visible:ring-inset"
                        >
                          <ChevronRightGlyph18
                            aria-hidden
                            className={cn(
                              "size-3.5 shrink-0 transition-transform",
                              open && "rotate-90",
                            )}
                          />
                          {title}
                        </button>
                      ) : (
                        <span className="flex items-center gap-1.5 pl-1.5">
                          {title}
                        </span>
                      )}
                    </h3>
                    {STAGE_ALL.has(section.id) && (
                      <TooltipIconButton
                        label={copy.changes.stageAll(
                          copy.changes.sections[section.id],
                        )}
                        className="mr-1 size-5 shrink-0"
                        onClick={() =>
                          void stageFiles(
                            path,
                            section.entries.map((entry) => entry.file.path),
                          )
                        }
                      >
                        <PlusGlyph18 />
                      </TooltipIconButton>
                    )}
                  </div>
                  {open && (
                    <div
                      id={listId}
                      role="tree"
                      aria-labelledby={`changes-${section.id}`}
                    >
                      {section.entries.map((entry) => (
                        <ChangeRow
                          key={rowKey(entry, section.id)}
                          entry={entry}
                          open={entry.file.path === activePath}
                          focusable={
                            sectionIndex === 0 && entry === section.entries[0]
                          }
                          index={indexActionFor(section.id, entry)}
                          onPreview={() => show(entry, true)}
                          onPin={() => show(entry, false)}
                        />
                      ))}
                    </div>
                  )}
                </div>
              );
            })}
          </div>
        </ScrollArea>
      )}
    </div>
  );
}
