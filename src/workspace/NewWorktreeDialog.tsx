import { useEffect, useState, type FormEvent } from "react";

import { commands, type RepoRefs } from "@/bindings";
import { Button } from "@/components/ui/button";
import {
  Dialog,
  DialogContent,
  DialogDescription,
  DialogFooter,
  DialogHeader,
  DialogTitle,
} from "@/components/ui/dialog";
import { Input } from "@/components/ui/input";
import { Label } from "@/components/ui/label";
import {
  Select,
  SelectContent,
  SelectItem,
  SelectTrigger,
  SelectValue,
} from "@/components/ui/select";
import { unwrap } from "@/lib/ipc-result";
import { copy } from "@/ui-copy";
import {
  useWorkspaceStore,
  type WorktreeDraft,
} from "@/workspace/workspace-store";

export function NewWorktreeDialog() {
  const draft = useWorkspaceStore((state) => state.draft);
  const closeNewWorktree = useWorkspaceStore((state) => state.closeNewWorktree);

  return (
    <Dialog
      open={draft !== null}
      onOpenChange={(open) => {
        if (!open) closeNewWorktree();
      }}
    >
      {/* Mounted only while a draft exists, so reopening after an error starts from what was typed. */}
      {draft && <NewWorktreeForm draft={draft} />}
    </Dialog>
  );
}

/** The base picker lists the default first, then every other branch. */
function baseOptions(refs: RepoRefs | null): string[] {
  if (!refs) return [];
  return [
    refs.defaultBase,
    ...refs.refs.filter((ref) => ref !== refs.defaultBase),
  ];
}

function NewWorktreeForm({ draft }: { draft: WorktreeDraft }) {
  const createWorktree = useWorkspaceStore((state) => state.createWorktree);
  const closeNewWorktree = useWorkspaceStore((state) => state.closeNewWorktree);

  const [name, setName] = useState(draft.name);
  const [baseRef, setBaseRef] = useState<string | null>(draft.baseRef);
  const [error, setError] = useState<string | null>(draft.error);
  const [refs, setRefs] = useState<RepoRefs | null>(null);

  useEffect(() => {
    let cancelled = false;
    unwrap(commands.repoRefs(draft.repoId))
      .then((loaded) => {
        if (!cancelled) setRefs(loaded);
      })
      // Without the list the backend still picks the default base.
      .catch(() => undefined);
    return () => {
      cancelled = true;
    };
  }, [draft.repoId]);

  const canSubmit = name.trim().length > 0;

  function submit(event: FormEvent) {
    event.preventDefault();
    if (!canSubmit) return;
    void createWorktree({ repoId: draft.repoId, name, baseRef, error: null });
  }

  return (
    <DialogContent className="sm:max-w-md">
      <form onSubmit={submit} className="grid gap-4">
        <DialogHeader>
          <DialogTitle>{copy.newWorktree.title}</DialogTitle>
          <DialogDescription>{copy.newWorktree.description}</DialogDescription>
        </DialogHeader>

        <div className="grid gap-1.5">
          <Label htmlFor="worktree-name">{copy.newWorktree.nameLabel}</Label>
          <Input
            id="worktree-name"
            value={name}
            autoComplete="off"
            spellCheck={false}
            placeholder={copy.newWorktree.namePlaceholder}
            aria-invalid={error !== null}
            aria-describedby="worktree-name-hint"
            onChange={(event) => {
              setName(event.target.value);
              setError(null);
            }}
          />
          {error ? (
            <p
              id="worktree-name-hint"
              role="alert"
              className="text-xs text-destructive"
            >
              {error}
            </p>
          ) : (
            <p
              id="worktree-name-hint"
              className="text-xs text-muted-foreground"
            >
              {copy.newWorktree.nameHint}
            </p>
          )}
        </div>

        <div className="grid gap-1.5">
          <Label htmlFor="worktree-base">{copy.newWorktree.baseLabel}</Label>
          <Select
            value={baseRef ?? refs?.defaultBase ?? ""}
            onValueChange={setBaseRef}
            disabled={refs === null}
          >
            <SelectTrigger id="worktree-base" className="w-full">
              <SelectValue placeholder={copy.newWorktree.loadingRefs} />
            </SelectTrigger>
            <SelectContent>
              {baseOptions(refs).map((ref) => (
                <SelectItem key={ref} value={ref}>
                  {ref}
                </SelectItem>
              ))}
            </SelectContent>
          </Select>
        </div>

        <DialogFooter>
          <Button type="button" variant="ghost" onClick={closeNewWorktree}>
            {copy.newWorktree.cancel}
          </Button>
          <Button type="submit" disabled={!canSubmit}>
            {copy.newWorktree.create}
          </Button>
        </DialogFooter>
      </form>
    </DialogContent>
  );
}
