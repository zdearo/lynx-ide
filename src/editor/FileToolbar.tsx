import type { DiffSource } from "@/bindings";
import { changeLetter } from "@/changes/change-sections";
import { changeOf, useChangesStore } from "@/changes/changes-store";
import { useDiffModeStore, type DiffMode } from "@/editor/diff-mode";
import {
  useEditorStore,
  type FileView,
  type OpenFile,
} from "@/editor/editor-store";
import { cn } from "@/lib/utils";
import { copy } from "@/ui-copy";

function Segment<T extends string>({
  label,
  value,
  options,
  onChange,
}: {
  label: string;
  value: T;
  options: { id: T; label: string; disabled?: boolean }[];
  onChange: (id: T) => void;
}) {
  return (
    <div
      role="group"
      aria-label={label}
      className="flex items-center gap-0.5 rounded-md bg-muted p-0.5"
    >
      {options.map((option) => (
        <button
          key={option.id}
          type="button"
          aria-pressed={option.id === value}
          disabled={option.disabled}
          onClick={() => onChange(option.id)}
          className={cn(
            "h-5 rounded-sm px-2 text-meta outline-none focus-visible:ring-2 focus-visible:ring-ring/60 disabled:pointer-events-none disabled:opacity-50",
            option.id === value
              ? "bg-background text-foreground"
              : "text-muted-foreground hover:text-foreground",
          )}
        >
          {option.label}
        </button>
      ))}
    </div>
  );
}

const COMPARE_KINDS = ["head", "staged", "unstaged", "base"] as const;

function compareSource(kind: DiffSource["kind"]): DiffSource {
  return kind === "commit" ? { kind: "head" } : { kind };
}

/** The strip above a file: where it is, whether it changed, and edit/diff switching. */
export function FileToolbar({
  worktreePath,
  file,
}: {
  worktreePath: string;
  file: OpenFile;
}) {
  const change = useChangesStore((state) =>
    changeOf(state.byWorktree[worktreePath], file.path),
  );
  const mode = useDiffModeStore((state) => state.mode);
  const setMode = useDiffModeStore((state) => state.setMode);
  const kind = change?.unstaged ?? change?.staged ?? null;

  // A tab of an old commit has an internal id; the toolbar shows the real path.
  const shownPath = file.commit?.path ?? file.path;
  const slash = shownPath.lastIndexOf("/");
  const folder = slash >= 0 ? shownPath.slice(0, slash + 1) : "";
  const shownName = slash >= 0 ? shownPath.slice(slash + 1) : shownPath;

  return (
    <div
      role="toolbar"
      aria-label={copy.editor.toolbar}
      className="flex h-8 shrink-0 items-center gap-3 border-b border-border bg-background px-3 text-ui"
    >
      <span
        className="flex min-w-0 flex-1 items-center gap-2"
        title={shownPath}
      >
        <span className="min-w-0 truncate">
          <span className="text-muted-foreground">{folder}</span>
          {shownName}
        </span>
        {kind && (
          <span
            title={copy.changes.kinds[kind]}
            className={cn(
              "shrink-0 text-meta font-medium tabular-nums",
              kind === "deleted" || kind === "conflicted"
                ? "text-diff-removed"
                : "text-diff-added",
            )}
          >
            {changeLetter(kind)}
            <span className="sr-only">{copy.changes.kinds[kind]}</span>
          </span>
        )}
      </span>

      {file.commit && (
        <span className="shrink-0 rounded-sm bg-muted px-1.5 text-meta font-mono text-muted-foreground">
          {copy.diff.commitTab(file.commit.hash)}
        </span>
      )}
      {file.view === "diff" && !file.commit && (
        <Segment<DiffSource["kind"]>
          label={copy.diff.compareGroup}
          value={file.diffSource.kind}
          onChange={(kind) =>
            useEditorStore
              .getState()
              .setDiffSource(worktreePath, file.path, compareSource(kind))
          }
          options={COMPARE_KINDS.map((id) => ({
            id,
            label: copy.diff.compare[id],
          }))}
        />
      )}
      {file.view === "diff" && (
        <Segment<DiffMode>
          label={copy.diff.modeGroup}
          value={mode}
          onChange={setMode}
          options={[
            { id: "unified", label: copy.diff.unified },
            { id: "split", label: copy.diff.split },
          ]}
        />
      )}
      <Segment<FileView>
        label={copy.editor.viewGroup}
        value={file.view}
        onChange={(view) =>
          useEditorStore.getState().setView(worktreePath, file.path, view)
        }
        options={[
          {
            id: "edit",
            label: copy.editor.viewEdit,
            disabled:
              Boolean(file.commit) ||
              file.imageUrl !== null ||
              (file.status === "error" && file.view !== "edit"),
          },
          {
            id: "diff",
            label: copy.editor.viewDiff,
            disabled:
              file.imageUrl !== null ||
              (!change && file.view !== "diff" && !file.commit),
          },
        ]}
      />
    </div>
  );
}
