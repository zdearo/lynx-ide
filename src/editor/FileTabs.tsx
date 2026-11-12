import type { KeyboardEvent } from "react";

import { XIcon } from "@phosphor-icons/react";
import { FileTypeIcon } from "@/files/FileTypeIcon";
import { requestCloseFile } from "@/editor/editor-actions";
import { useEditorStore } from "@/editor/editor-store";
import { cn } from "@/lib/utils";
import { copy } from "@/ui-copy";

/** The strip under the terminal tabs: the files open in this worktree. */
export function FileTabs({ worktreePath }: { worktreePath: string }) {
  const files = useEditorStore((state) => state.files[worktreePath]);
  const activePath = useEditorStore((state) => state.active[worktreePath]);
  if (!files?.length) return null;

  function moveTab(event: KeyboardEvent<HTMLDivElement>) {
    if (!files || (event.key !== "ArrowRight" && event.key !== "ArrowLeft"))
      return;
    const index = files.findIndex((file) => file.path === activePath);
    const step = event.key === "ArrowRight" ? 1 : -1;
    const next = files[(index + step + files.length) % files.length];
    if (!next) return;
    event.preventDefault();
    useEditorStore.getState().show(worktreePath, next.path);
    document.getElementById(`file-tab-${next.path}`)?.focus();
  }

  return (
    <div
      role="tablist"
      aria-label={copy.editor.tabs}
      onKeyDown={moveTab}
      className="flex h-8 shrink-0 items-stretch overflow-x-auto border-t border-border bg-sidebar"
    >
      {files.map((file) => {
        const active = file.path === activePath;
        return (
          <button
            key={file.path}
            type="button"
            role="tab"
            id={`file-tab-${file.path}`}
            aria-selected={active}
            aria-keyshortcuts="Delete"
            tabIndex={active ? 0 : -1}
            title={file.path}
            onDoubleClick={() =>
              useEditorStore.getState().pin(worktreePath, file.path)
            }
            onClick={(event) => {
              if (
                event.target instanceof Element &&
                event.target.closest("[data-close]")
              ) {
                requestCloseFile(worktreePath, file.path);
              } else {
                useEditorStore.getState().show(worktreePath, file.path);
              }
            }}
            onKeyDown={(event) => {
              if (event.key === "Delete") {
                event.preventDefault();
                requestCloseFile(worktreePath, file.path);
              }
            }}
            className={cn(
              "group/tab flex h-full max-w-48 shrink-0 items-center gap-1.5 border-r border-border pr-1 pl-3 text-ui [&_svg]:size-3.5",
              active
                ? "bg-background text-foreground"
                : "text-muted-foreground hover:text-foreground",
            )}
          >
            <FileTypeIcon name={file.name} isDirectory={false} open={false} />
            <span className={cn("truncate", file.preview && "italic")}>
              {file.name}
            </span>
            {file.preview && (
              <span className="sr-only">{copy.editor.preview}</span>
            )}
            <span
              data-close
              aria-hidden
              title={copy.editor.closeNamed(file.name)}
              className={cn(
                "grid size-5 shrink-0 place-items-center rounded-md hover:bg-muted",
                !active &&
                  !file.dirty &&
                  "opacity-0 group-focus-visible/tab:opacity-100 group-hover/tab:opacity-100",
              )}
            >
              {file.dirty ? (
                <span className="size-1.5 rounded-full bg-foreground" />
              ) : (
                <XIcon />
              )}
            </span>
            {file.dirty && (
              <span className="sr-only">{copy.editor.unsaved}</span>
            )}
          </button>
        );
      })}
    </div>
  );
}
