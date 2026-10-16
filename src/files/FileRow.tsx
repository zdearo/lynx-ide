import { ChevronRightGlyph18 } from "@/components/icons/ChevronRightGlyph18";
import { DotsGlyph18 } from "@/components/icons/DotsGlyph18";
import { Link2Glyph18 } from "@/components/icons/Link2Glyph18";
import { Loader2Glyph18 } from "@/components/icons/Loader2Glyph18";

import { TooltipIconButton } from "@/components/TooltipIconButton";
import {
  DropdownMenu,
  DropdownMenuContent,
  DropdownMenuItem,
  DropdownMenuSeparator,
  DropdownMenuTrigger,
} from "@/components/ui/dropdown-menu";
import {
  absolutePath,
  copyToClipboard,
  openContainingFolder,
  sendPathToTerminal,
} from "@/files/file-actions";
import { FileTypeIcon } from "@/files/FileTypeIcon";
import type { EntryRow } from "@/files/file-tree-rows";
import { cn } from "@/lib/utils";
import { copy } from "@/ui-copy";

/** The row's accessible name: without this it would also read the actions button inside it. */
function rowLabel(row: EntryRow): string {
  const notes = [
    row.symlink ? copy.files.link : null,
    row.ignored ? copy.files.ignored : null,
  ].filter(Boolean);
  return notes.length ? `${row.name}, ${notes.join(", ")}` : row.name;
}

const INDENT_PX = 12;
const BASE_PADDING_PX = 6;

interface FileRowProps {
  row: EntryRow;
  worktreePath: string;
  selected: boolean;
  /** The one row that is a tab stop; the arrow keys move it. */
  focusable: boolean;
  menuOpen: boolean;
  onMenuOpenChange: (open: boolean) => void;
  onActivate: () => void;
}

export function FileRow({
  row,
  worktreePath,
  selected,
  focusable,
  menuOpen,
  onMenuOpenChange,
  onActivate,
}: FileRowProps) {
  return (
    <div
      role="treeitem"
      aria-label={rowLabel(row)}
      data-tree-path={row.path}
      aria-level={row.depth + 1}
      aria-expanded={row.isDirectory ? row.expanded : undefined}
      aria-selected={selected}
      tabIndex={focusable ? 0 : -1}
      onClick={onActivate}
      onContextMenu={(event) => {
        event.preventDefault();
        onActivate();
        onMenuOpenChange(true);
      }}
      style={{ paddingLeft: BASE_PADDING_PX + row.depth * INDENT_PX }}
      className={cn(
        "group/file relative flex h-6 cursor-default items-center gap-1 rounded-md pr-1 text-ui outline-none",
        "focus-visible:ring-2 focus-visible:ring-ring/60 focus-visible:ring-inset",
        selected
          ? "bg-sidebar-accent text-sidebar-accent-foreground"
          : "hover:bg-sidebar-accent/50",
        row.ignored ? "text-muted-foreground" : "text-sidebar-foreground",
      )}
    >
      {row.isDirectory ? (
        row.loading ? (
          <Loader2Glyph18
            aria-hidden
            className="size-3.5 shrink-0 animate-spin text-muted-foreground"
          />
        ) : (
          <ChevronRightGlyph18
            aria-hidden
            className={cn(
              "size-3.5 shrink-0 text-muted-foreground transition-transform",
              row.expanded && "rotate-90",
            )}
          />
        )
      ) : (
        <span aria-hidden className="size-3.5 shrink-0" />
      )}
      <FileTypeIcon
        name={row.name}
        isDirectory={row.isDirectory}
        open={row.expanded}
        className="text-muted-foreground"
      />
      <span className="min-w-0 flex-1 truncate" title={row.path}>
        {row.name}
      </span>
      {row.symlink && (
        <Link2Glyph18
          aria-hidden
          className="size-3 shrink-0 text-muted-foreground"
        />
      )}

      <div
        className={cn(
          "absolute top-0.5 right-1 rounded-md bg-sidebar-accent opacity-0 transition-opacity",
          "group-focus-within/file:opacity-100 group-hover/file:opacity-100",
          menuOpen && "opacity-100",
        )}
      >
        <DropdownMenu open={menuOpen} onOpenChange={onMenuOpenChange}>
          <DropdownMenuTrigger asChild>
            <TooltipIconButton
              label={copy.files.actions(row.name)}
              tabIndex={-1}
              className="size-5"
              onClick={(event) => event.stopPropagation()}
            >
              <DotsGlyph18 />
            </TooltipIconButton>
          </DropdownMenuTrigger>
          <DropdownMenuContent align="end" className="min-w-52">
            <DropdownMenuItem
              onSelect={() => void sendPathToTerminal(worktreePath, row.path)}
            >
              {copy.files.sendToTerminal}
            </DropdownMenuItem>
            <DropdownMenuSeparator />
            <DropdownMenuItem
              onSelect={() =>
                void copyToClipboard(absolutePath(worktreePath, row.path))
              }
            >
              {copy.files.copyPath}
            </DropdownMenuItem>
            <DropdownMenuItem onSelect={() => void copyToClipboard(row.path)}>
              {copy.files.copyRelativePath}
            </DropdownMenuItem>
            <DropdownMenuItem
              onSelect={() => void openContainingFolder(worktreePath, row.path)}
            >
              {copy.files.openFolder}
            </DropdownMenuItem>
          </DropdownMenuContent>
        </DropdownMenu>
      </div>
    </div>
  );
}
