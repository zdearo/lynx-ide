import type { ReactNode } from "react";
import { ChevronRightGlyph18 } from "@/components/icons/ChevronRightGlyph18";
import { DotsGlyph18 } from "@/components/icons/DotsGlyph18";
import { FolderGlyph18 } from "@/components/icons/FolderGlyph18";
import { FolderOpenGlyph18 } from "@/components/icons/FolderOpenGlyph18";
import { FolderPlusGlyph18 } from "@/components/icons/FolderPlusGlyph18";
import { toast } from "sonner";

import type { Folder } from "@/bindings";
import { TooltipIconButton } from "@/components/TooltipIconButton";
import {
  DropdownMenu,
  DropdownMenuContent,
  DropdownMenuItem,
  DropdownMenuSeparator,
  DropdownMenuTrigger,
} from "@/components/ui/dropdown-menu";
import { useDialogStore } from "@/dialog-store";
import { errorMessage } from "@/lib/error-message";
import { cn } from "@/lib/utils";
import { copy } from "@/ui-copy";
import { CollapsedSummary } from "@/workspace/CollapsedSummary";
import type { GroupSummary } from "@/workspace/collapsed-summary";
import { useWorkspaceStore } from "@/workspace/workspace-store";

interface FolderSectionProps {
  folder: Folder;
  /** Top-level folders read as groups: a label with no icon, set apart from the next group. */
  root: boolean;
  summary: GroupSummary;
  /** Nothing inside: say so instead of drawing an empty list. */
  empty: boolean;
  children: ReactNode;
}

/** A folder in the project tree. Its contents are indented under it. */
export function FolderSection({
  folder,
  root,
  summary,
  empty,
  children,
}: FolderSectionProps) {
  const summaryId = `folder-summary-${folder.id}`;
  const setFolderCollapsed = useWorkspaceStore(
    (state) => state.setFolderCollapsed,
  );
  const askFolderDialog = useDialogStore((state) => state.askFolderDialog);
  const askMove = useDialogStore((state) => state.askMove);
  const askDeleteFolder = useDialogStore((state) => state.askDeleteFolder);
  const expanded = !folder.collapsed;

  async function toggle() {
    try {
      await setFolderCollapsed(folder.id, expanded);
    } catch (error) {
      toast.error(copy.folders.failed, { description: errorMessage(error) });
    }
  }

  return (
    <li data-folder-id={folder.id} className={cn(root && "mt-3 first:mt-0")}>
      <div className="group/folder relative">
        <button
          type="button"
          data-nav-item
          aria-expanded={expanded}
          aria-describedby={expanded ? undefined : summaryId}
          aria-label={
            expanded
              ? copy.folders.collapse(folder.name)
              : copy.folders.expand(folder.name)
          }
          onClick={() => void toggle()}
          className="flex h-7 w-full items-center gap-1.5 rounded-md px-1.5 text-left text-ui text-sidebar-foreground hover:bg-sidebar-accent/50"
        >
          {!root &&
            (expanded ? (
              <FolderOpenGlyph18
                aria-hidden
                className="size-3.5 shrink-0 text-muted-foreground"
              />
            ) : (
              <FolderGlyph18
                aria-hidden
                className="size-3.5 shrink-0 text-muted-foreground"
              />
            ))}
          <span
            className={cn(
              "min-w-0 flex-1 truncate",
              root &&
                "text-meta font-medium tracking-wide text-muted-foreground uppercase",
              !root && "text-sidebar-foreground/80",
            )}
            title={folder.name}
          >
            {folder.name}
          </span>
          {!expanded && (
            <CollapsedSummary id={summaryId} kind="folder" summary={summary} />
          )}
        </button>

        <div className="pointer-events-none absolute top-0.5 right-1 flex items-center gap-0.5 rounded-md bg-sidebar opacity-0 transition-opacity group-has-[:focus-visible]/folder:opacity-100 group-hover/folder:opacity-100">
          <TooltipIconButton
            className="pointer-events-auto"
            label={copy.folders.newFolder}
            onClick={() =>
              askFolderDialog({ mode: "create", parentId: folder.id })
            }
          >
            <FolderPlusGlyph18 />
          </TooltipIconButton>
          <DropdownMenu>
            <DropdownMenuTrigger asChild>
              <TooltipIconButton
                label={copy.folders.actions(folder.name)}
                className="pointer-events-auto"
              >
                <DotsGlyph18 />
              </TooltipIconButton>
            </DropdownMenuTrigger>
            <DropdownMenuContent align="end" className="min-w-44">
              <DropdownMenuItem
                onSelect={() => askFolderDialog({ mode: "rename", folder })}
              >
                {copy.folders.rename}
              </DropdownMenuItem>
              <DropdownMenuItem
                onSelect={() => askMove({ kind: "folder", folder })}
              >
                {copy.folders.moveTo}
              </DropdownMenuItem>
              <DropdownMenuSeparator />
              <DropdownMenuItem
                variant="destructive"
                onSelect={() => askDeleteFolder(folder)}
              >
                {copy.folders.delete}
              </DropdownMenuItem>
            </DropdownMenuContent>
          </DropdownMenu>
          <span
            aria-hidden
            className="pointer-events-none grid size-6 place-items-center text-muted-foreground"
          >
            <ChevronRightGlyph18
              data-icon="chevron-right"
              className={cn(
                "size-3.5 transition-transform",
                expanded && "rotate-90",
              )}
            />
          </span>
        </div>
      </div>

      {expanded &&
        (empty ? (
          <p className="ml-7 py-0.5 text-meta text-muted-foreground">
            {copy.folders.empty}
          </p>
        ) : (
          <ul className="ml-3">{children}</ul>
        ))}
    </li>
  );
}
