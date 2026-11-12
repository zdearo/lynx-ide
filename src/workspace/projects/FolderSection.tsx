import type { ReactNode } from "react";
import {
  FolderIcon,
  FolderOpenIcon,
  FolderPlusIcon,
  LinkIcon,
} from "@phosphor-icons/react";
import { CaretRightIcon, DotsThreeIcon } from "@/components/icons/line-icons";
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
import { CollapsedSummary } from "@/workspace/projects/CollapsedSummary";
import type { GroupSummary } from "@/workspace/projects/collapsed-summary";
import { isLinkedRoot, isMirrored } from "@/workspace/projects/linked-folders";
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
  const askUnlinkFolder = useDialogStore((state) => state.askUnlinkFolder);
  const syncLinkedFolders = useWorkspaceStore(
    (state) => state.syncLinkedFolders,
  );
  const linked = isLinkedRoot(folder);
  const mirrored = isMirrored(folder);
  const expanded = !folder.collapsed;

  async function toggle() {
    try {
      await setFolderCollapsed(folder.id, expanded);
    } catch (error) {
      toast.error(copy.folders.failed, { description: errorMessage(error) });
    }
  }

  async function rescan() {
    try {
      const changed = await syncLinkedFolders(folder.id);
      toast.success(
        changed ? copy.folders.rescanFound : copy.folders.rescanned,
      );
    } catch (error) {
      toast.error(copy.folders.syncFailed, {
        description: errorMessage(error),
      });
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
          className={cn(
            "flex h-7 w-full items-center gap-1.5 rounded-md px-1.5 text-left text-ui text-sidebar-foreground hover:bg-sidebar-accent/50",
          )}
        >
          {!root &&
            (expanded ? (
              <FolderOpenIcon
                aria-hidden
                className="size-3.5 shrink-0 text-muted-foreground"
              />
            ) : (
              <FolderIcon
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
              folder.missing && "text-muted-foreground",
            )}
            title={
              linked && folder.sourcePath
                ? copy.folders.linkedTo(folder.sourcePath)
                : folder.missing && folder.sourcePath
                  ? copy.folders.notFoundHint(folder.sourcePath)
                  : folder.name
            }
          >
            {folder.name}
          </span>
          {linked && (
            <LinkIcon
              aria-hidden
              className="size-3 shrink-0 text-muted-foreground"
            />
          )}
          {folder.missing && (
            <span className="shrink-0 text-meta text-muted-foreground">
              {copy.folders.notFound}
            </span>
          )}
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
            <FolderPlusIcon />
          </TooltipIconButton>
          <DropdownMenu>
            <DropdownMenuTrigger asChild>
              <TooltipIconButton
                label={copy.folders.actions(folder.name)}
                className="pointer-events-auto"
              >
                <DotsThreeIcon />
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
              {linked && (
                <>
                  <DropdownMenuItem onSelect={() => void rescan()}>
                    {copy.folders.rescan}
                  </DropdownMenuItem>
                  <DropdownMenuItem onSelect={() => askUnlinkFolder(folder)}>
                    {copy.folders.unlink}
                  </DropdownMenuItem>
                </>
              )}
              {!mirrored && (
                <>
                  <DropdownMenuSeparator />
                  <DropdownMenuItem
                    variant="destructive"
                    onSelect={() => askDeleteFolder(folder)}
                  >
                    {copy.folders.delete}
                  </DropdownMenuItem>
                </>
              )}
            </DropdownMenuContent>
          </DropdownMenu>
          <span
            aria-hidden
            className="pointer-events-none grid size-6 place-items-center text-muted-foreground"
          >
            <CaretRightIcon
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
          <p className="ml-3 py-0.5 pl-1.5 text-meta text-muted-foreground">
            {copy.folders.empty}
          </p>
        ) : (
          <ul>{children}</ul>
        ))}
    </li>
  );
}
