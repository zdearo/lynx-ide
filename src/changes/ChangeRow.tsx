import { ChangeMark } from "@/changes/ChangeMark";
import type { ChangeEntry } from "@/changes/change-sections";
import { FileTypeIcon } from "@/files/FileTypeIcon";
import { cn } from "@/lib/utils";
import { copy } from "@/ui-copy";

const INDENT_PX = 6;

interface ChangeRowProps {
  entry: ChangeEntry;
  /** The tab that shows this file now. */
  open: boolean;
  /** The one row that is a tab stop; the arrow keys move it. */
  focusable: boolean;
  onPreview: () => void;
  onPin: () => void;
}

/** One changed file: icon, name with its folder, and the status letter on the right. */
export function ChangeRow({
  entry,
  open,
  focusable,
  onPreview,
  onPin,
}: ChangeRowProps) {
  const { file, kind } = entry;
  const slash = file.path.lastIndexOf("/");
  const name = slash >= 0 ? file.path.slice(slash + 1) : file.path;
  const folder = slash >= 0 ? file.path.slice(0, slash) : "";
  const label = `${name}, ${copy.changes.kinds[kind]}`;

  return (
    <div
      role="option"
      aria-selected={open}
      aria-label={label}
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
        }
      }}
      style={{ paddingLeft: INDENT_PX }}
      title={
        file.originalPath
          ? `${file.path} (${copy.changes.renamedFrom(file.originalPath)})`
          : file.path
      }
      className={cn(
        "flex h-6 cursor-default items-center gap-1.5 rounded-md pr-2 text-ui outline-none",
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
      <ChangeMark kind={kind} className="ml-auto" />
    </div>
  );
}
