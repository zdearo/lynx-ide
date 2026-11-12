import { ArrowUUpLeftIcon } from "@phosphor-icons/react";
import { MinusIcon, PlusIcon } from "@/components/icons/line-icons";
import { TooltipIconButton } from "@/components/TooltipIconButton";
import { ChangeMark } from "@/changes/ChangeMark";
import type { ChangeEntry } from "@/changes/change-sections";
import { FileTypeIcon } from "@/files/FileTypeIcon";
import { cn } from "@/lib/utils";
import { copy } from "@/ui-copy";

const INDENT_PX = 6;

/** Moves a file into the index (stage) or back out of it (unstage). */
export interface IndexAction {
  kind: "stage" | "unstage";
  onRun: () => void;
}

interface ChangeRowProps {
  entry: ChangeEntry;
  /** The tab that shows this file now. */
  open: boolean;
  /** The one row that is a tab stop; the arrow keys move it. */
  focusable: boolean;
  /** Absent where the file cannot be staged, such as a conflict. */
  index?: IndexAction;
  /** Asks to throw away this file's unstaged change. Absent for staged and conflicted files. */
  onDiscard?: () => void;
  onPreview: () => void;
  onPin: () => void;
}

/** One changed file: icon, name with its folder, then on the right the actions, the lines added and removed, and the status letter. */
export function ChangeRow({
  entry,
  open,
  focusable,
  index,
  onDiscard,
  onPreview,
  onPin,
}: ChangeRowProps) {
  const { file, kind, lines } = entry;
  const slash = file.path.lastIndexOf("/");
  const name = slash >= 0 ? file.path.slice(slash + 1) : file.path;
  const folder = slash >= 0 ? file.path.slice(0, slash) : "";
  const label = `${name}, ${copy.changes.kinds[kind]}`;

  return (
    <div
      role="treeitem"
      aria-selected={open}
      aria-label={label}
      aria-keyshortcuts={
        [index && "S", onDiscard && "Delete"].filter(Boolean).join(" ") ||
        undefined
      }
      data-change-path={file.path}
      tabIndex={focusable ? 0 : -1}
      onClick={onPreview}
      onDoubleClick={onPin}
      onKeyDown={(event) => {
        if (event.key === "Enter") {
          event.preventDefault();
          onPin();
        } else if (event.key === " ") {
          event.preventDefault();
          onPreview();
        } else if (index && (event.key === "s" || event.key === "S")) {
          event.preventDefault();
          index.onRun();
        } else if (onDiscard && event.key === "Delete") {
          event.preventDefault();
          onDiscard();
        }
      }}
      style={{ paddingLeft: INDENT_PX }}
      title={
        file.originalPath
          ? `${file.path} (${copy.changes.renamedFrom(file.originalPath)})`
          : file.path
      }
      className={cn(
        "group/change flex h-6 cursor-default items-center gap-1.5 rounded-md pr-2 text-ui outline-none",
        "focus-visible:ring-2 focus-visible:ring-ring/60 focus-visible:ring-inset",
        open
          ? "bg-sidebar-accent text-sidebar-accent-foreground"
          : "text-sidebar-foreground hover:bg-sidebar-accent/50",
      )}
    >
      <FileTypeIcon
        name={name}
        isDirectory={false}
        open={false}
        className="text-muted-foreground"
      />
      <span
        className={cn(
          "min-w-0 truncate",
          kind === "deleted" && "line-through decoration-1",
        )}
      >
        {name}
        {folder && (
          <span className="ml-1.5 text-meta text-muted-foreground">
            {folder}
          </span>
        )}
      </span>
      <div className="ml-auto flex shrink-0 items-center gap-1">
        {onDiscard && (
          <TooltipIconButton
            label={copy.changes.discard.file(name)}
            tabIndex={-1}
            className="size-5 opacity-0 group-hover/change:opacity-100 focus-visible:opacity-100"
            onClick={(event) => {
              event.stopPropagation();
              onDiscard();
            }}
            onDoubleClick={(event) => event.stopPropagation()}
          >
            <ArrowUUpLeftIcon />
          </TooltipIconButton>
        )}
        {index && (
          // Keyboard users stage with S on the row; this button is the mouse path.
          <TooltipIconButton
            label={
              index.kind === "stage"
                ? copy.changes.stage(name)
                : copy.changes.unstage(name)
            }
            tabIndex={-1}
            className="size-5 opacity-0 group-hover/change:opacity-100 focus-visible:opacity-100"
            onClick={(event) => {
              event.stopPropagation();
              index.onRun();
            }}
            onDoubleClick={(event) => event.stopPropagation()}
          >
            {index.kind === "stage" ? <PlusIcon /> : <MinusIcon />}
          </TooltipIconButton>
        )}
        {lines && (lines.added > 0 || lines.removed > 0) && (
          <span
            className="text-meta tabular-nums"
            title={copy.changes.lines(lines.added, lines.removed)}
          >
            {lines.added > 0 && (
              <span className="text-diff-added">+{lines.added}</span>
            )}
            {lines.added > 0 && lines.removed > 0 && " "}
            {lines.removed > 0 && (
              <span className="text-diff-removed">−{lines.removed}</span>
            )}
          </span>
        )}
        <ChangeMark kind={kind} />
      </div>
    </div>
  );
}
