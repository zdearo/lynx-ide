import { useState, type FormEvent, type KeyboardEvent } from "react";

import { Button } from "@/components/ui/button";
import {
  Dialog,
  DialogContent,
  DialogDescription,
  DialogFooter,
  DialogHeader,
  DialogTitle,
} from "@/components/ui/dialog";
import { useDialogStore, type MoveTarget } from "@/dialog-store";
import { errorMessage } from "@/lib/error-message";
import { cn } from "@/lib/utils";
import { copy } from "@/ui-copy";
import {
  flattenFolders,
  folderAndDescendantIds,
} from "@/workspace/project-tree";
import { useWorkspaceStore } from "@/workspace/workspace-store";

const TOP = "top";

function targetName(target: MoveTarget): string {
  return target.kind === "repo" ? target.repo.name : target.folder.name;
}

/** Picks where a repo or folder goes: the top level or any folder. A folder cannot go inside itself. */
export function MoveDialog() {
  const target = useDialogStore((state) => state.moveTarget);
  const askMove = useDialogStore((state) => state.askMove);

  return (
    <Dialog
      open={target !== null}
      onOpenChange={(open) => {
        if (!open) askMove(null);
      }}
    >
      {target && (
        <MoveForm
          key={targetName(target)}
          target={target}
          onDone={() => askMove(null)}
        />
      )}
    </Dialog>
  );
}

function MoveForm({
  target,
  onDone,
}: {
  target: MoveTarget;
  onDone: () => void;
}) {
  const folders = useWorkspaceStore((state) => state.folders);
  const moveRepo = useWorkspaceStore((state) => state.moveRepo);
  const moveFolder = useWorkspaceStore((state) => state.moveFolder);

  const currentParent =
    target.kind === "repo" ? target.repo.folderId : target.folder.parentId;
  const [choice, setChoice] = useState<string | null>(currentParent);
  const [error, setError] = useState<string | null>(null);
  const [busy, setBusy] = useState(false);

  const blocked =
    target.kind === "folder"
      ? folderAndDescendantIds(folders, target.folder.id)
      : new Set<string>();
  const options = [
    { id: null as string | null, label: copy.move.topLevel, depth: 0 },
    ...flattenFolders(folders).map(({ folder, depth }) => ({
      id: folder.id as string | null,
      label: folder.name,
      depth,
    })),
  ];
  const isBlocked = (id: string | null) => id !== null && blocked.has(id);
  const unchanged = choice === currentParent;
  const name = targetName(target);

  async function submit(event: FormEvent) {
    event.preventDefault();
    if (unchanged || busy) return;
    setBusy(true);
    setError(null);
    try {
      if (target.kind === "repo") await moveRepo(target.repo.id, choice);
      else await moveFolder(target.folder.id, choice);
      onDone();
    } catch (failure) {
      setError(errorMessage(failure));
    } finally {
      setBusy(false);
    }
  }

  // Radio group: the arrow keys move the choice and skip places the folder cannot go.
  function moveWithArrows(event: KeyboardEvent<HTMLDivElement>) {
    if (event.key !== "ArrowDown" && event.key !== "ArrowUp") return;
    const enabled = options.filter((option) => !isBlocked(option.id));
    const index = enabled.findIndex((option) => option.id === choice);
    const step = event.key === "ArrowDown" ? 1 : -1;
    const next = enabled[(index + step + enabled.length) % enabled.length];
    if (!next) return;
    event.preventDefault();
    setChoice(next.id);
    event.currentTarget
      .querySelector<HTMLElement>(`[data-option="${next.id ?? TOP}"]`)
      ?.focus();
  }

  return (
    <DialogContent className="sm:max-w-md">
      <form onSubmit={submit} className="grid gap-4">
        <DialogHeader>
          <DialogTitle>{copy.move.title(name)}</DialogTitle>
          <DialogDescription>{copy.move.description}</DialogDescription>
        </DialogHeader>

        <div
          role="radiogroup"
          aria-label={copy.move.title(name)}
          onKeyDown={moveWithArrows}
          className="grid max-h-72 gap-0.5 overflow-y-auto"
        >
          {options.map((option) => {
            const checked = choice === option.id;
            const disabled = isBlocked(option.id);
            return (
              <button
                key={option.id ?? TOP}
                type="button"
                role="radio"
                data-option={option.id ?? TOP}
                aria-checked={checked}
                disabled={disabled}
                tabIndex={checked ? 0 : -1}
                onClick={() => setChoice(option.id)}
                style={{ paddingLeft: 8 + option.depth * 12 }}
                className={cn(
                  "flex h-7 items-center gap-2 rounded-md pr-2 text-left text-ui",
                  checked
                    ? "bg-sidebar-accent text-foreground"
                    : "text-muted-foreground hover:bg-sidebar-accent/50 hover:text-foreground",
                  disabled && "opacity-40",
                )}
              >
                <span className="min-w-0 flex-1 truncate">{option.label}</span>
                {option.id === currentParent && (
                  <span className="shrink-0 text-meta text-muted-foreground">
                    {copy.move.current}
                  </span>
                )}
                {disabled && (
                  <span className="shrink-0 text-meta text-muted-foreground">
                    {copy.move.cannotMoveInside}
                  </span>
                )}
              </button>
            );
          })}
        </div>

        {error && (
          <p role="alert" className="text-xs text-destructive">
            {error}
          </p>
        )}

        <DialogFooter>
          <Button type="button" variant="ghost" onClick={onDone}>
            {copy.folders.cancel}
          </Button>
          <Button type="submit" disabled={unchanged || busy}>
            {copy.move.confirm}
          </Button>
        </DialogFooter>
      </form>
    </DialogContent>
  );
}
