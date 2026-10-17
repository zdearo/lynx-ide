import type { ChangeKind } from "@/bindings";
import {
  changeLetter,
  changeTone,
  type ChangeTone,
} from "@/changes/change-sections";
import { cn } from "@/lib/utils";

const TONE_TEXT: Record<ChangeTone, string> = {
  added: "text-diff-added",
  modified: "text-diff-modified",
  removed: "text-diff-removed",
};

/** The status letter at the right of a changed row. The row's name says the kind as well. */
export function ChangeMark({
  kind,
  className,
}: {
  kind: ChangeKind;
  className?: string;
}) {
  return (
    <span
      aria-hidden
      className={cn(
        "shrink-0 text-meta font-medium tabular-nums",
        TONE_TEXT[changeTone(kind)],
        className,
      )}
    >
      {changeLetter(kind)}
    </span>
  );
}

/** A folder that holds changes: a dot, since no single letter describes what is inside. */
export function FolderChangeMark() {
  return (
    <span
      aria-hidden
      className="size-1.5 shrink-0 rounded-full bg-diff-modified"
    />
  );
}
