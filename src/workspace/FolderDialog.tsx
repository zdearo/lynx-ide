import { useState, type FormEvent } from "react";

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
  useDialogStore,
  type FolderDialog as FolderDialogRequest,
} from "@/dialog-store";
import { errorMessage } from "@/lib/error-message";
import { copy } from "@/ui-copy";
import { useWorkspaceStore } from "@/workspace/workspace-store";

/** Creates a folder (inside a parent, or at the top) or renames one. Open while a request exists. */
export function FolderDialog() {
  const request = useDialogStore((state) => state.folderDialog);
  const askFolderDialog = useDialogStore((state) => state.askFolderDialog);

  return (
    <Dialog
      open={request !== null}
      onOpenChange={(open) => {
        if (!open) askFolderDialog(null);
      }}
    >
      {request && (
        <FolderForm request={request} onDone={() => askFolderDialog(null)} />
      )}
    </Dialog>
  );
}

function FolderForm({
  request,
  onDone,
}: {
  request: FolderDialogRequest;
  onDone: () => void;
}) {
  const createFolder = useWorkspaceStore((state) => state.createFolder);
  const renameFolder = useWorkspaceStore((state) => state.renameFolder);
  const [name, setName] = useState(
    request.mode === "rename" ? request.folder.name : "",
  );
  const [error, setError] = useState<string | null>(null);
  const [busy, setBusy] = useState(false);
  const canSubmit = name.trim().length > 0 && !busy;

  async function submit(event: FormEvent) {
    event.preventDefault();
    if (!canSubmit) return;
    setBusy(true);
    setError(null);
    try {
      if (request.mode === "create") {
        await createFolder(name, request.parentId);
      } else {
        await renameFolder(request.folder.id, name);
      }
      onDone();
    } catch (failure) {
      setError(errorMessage(failure));
    } finally {
      setBusy(false);
    }
  }

  const isRename = request.mode === "rename";

  return (
    <DialogContent className="sm:max-w-md">
      <form onSubmit={submit} className="grid gap-4">
        <DialogHeader>
          <DialogTitle>
            {isRename ? copy.folders.renameTitle : copy.folders.createTitle}
          </DialogTitle>
          <DialogDescription>{copy.folders.formDescription}</DialogDescription>
        </DialogHeader>

        <div className="grid gap-1.5">
          <Label htmlFor="folder-name">{copy.folders.nameLabel}</Label>
          <Input
            id="folder-name"
            value={name}
            autoComplete="off"
            placeholder={copy.folders.namePlaceholder}
            aria-invalid={error !== null}
            aria-describedby={error ? "folder-name-error" : undefined}
            onChange={(event) => {
              setName(event.target.value);
              setError(null);
            }}
          />
          {error && (
            <p
              id="folder-name-error"
              role="alert"
              className="text-xs text-destructive"
            >
              {error}
            </p>
          )}
        </div>

        <DialogFooter>
          <Button type="button" variant="ghost" onClick={onDone}>
            {copy.folders.cancel}
          </Button>
          <Button type="submit" disabled={!canSubmit}>
            {isRename ? copy.folders.save : copy.folders.create}
          </Button>
        </DialogFooter>
      </form>
    </DialogContent>
  );
}
