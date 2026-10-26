import { useRef, useState, type FormEvent } from "react";
import { open as pickFolder } from "@tauri-apps/plugin-dialog";

import { commands, type LinkedFolderPreview } from "@/bindings";
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
import { unwrap } from "@/lib/ipc-result";
import { cn } from "@/lib/utils";
import { copy } from "@/ui-copy";
import { directoryNameOf } from "@/workspace/projects/linked-folders";
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

/** What the chosen directory would bring in, or why it cannot be linked. */
type LinkCheck =
  | { state: "checking" }
  | { state: "ready"; preview: LinkedFolderPreview }
  | { state: "refused"; reason: string };

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
  const [sourcePath, setSourcePath] = useState<string | null>(null);
  const [check, setCheck] = useState<LinkCheck | null>(null);
  const [error, setError] = useState<string | null>(null);
  const latestCheck = useRef(0);
  const [busy, setBusy] = useState(false);
  const linkReady = sourcePath === null || check?.state === "ready";
  const canSubmit = name.trim().length > 0 && linkReady && !busy;

  async function chooseDirectory() {
    try {
      const chosen = await pickFolder({ directory: true, multiple: false });
      if (typeof chosen !== "string") return;
      setSourcePath(chosen);
      setError(null);
      if (name.trim().length === 0) setName(directoryNameOf(chosen));
      const thisCheck = ++latestCheck.current;
      setCheck({ state: "checking" });
      try {
        const preview = await unwrap(commands.previewLinkedFolder(chosen));
        // A newer choice (or "Don't link") may have replaced this one while the scan ran.
        if (thisCheck === latestCheck.current) {
          setCheck({ state: "ready", preview });
        }
      } catch (failure) {
        if (thisCheck === latestCheck.current) {
          setCheck({ state: "refused", reason: errorMessage(failure) });
        }
      }
    } catch (failure) {
      setError(errorMessage(failure));
    }
  }

  function clearDirectory() {
    latestCheck.current++;
    setSourcePath(null);
    setCheck(null);
  }

  async function submit(event: FormEvent) {
    event.preventDefault();
    if (!canSubmit) return;
    setBusy(true);
    setError(null);
    try {
      if (request.mode === "create") {
        await createFolder(name, request.parentId, sourcePath);
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

        {!isRename && (
          <div className="grid gap-1.5">
            <Label>{copy.folders.linkLabel}</Label>
            <div className="flex items-center gap-3">
              <p
                id="folder-link-path"
                className={cn(
                  "min-w-0 flex-1 text-ui break-all",
                  sourcePath ? "text-foreground" : "text-muted-foreground",
                )}
              >
                {sourcePath ?? copy.folders.linkNone}
              </p>
              {sourcePath && (
                <Button type="button" variant="ghost" onClick={clearDirectory}>
                  {copy.folders.linkClear}
                </Button>
              )}
              <Button
                type="button"
                variant="outline"
                aria-describedby="folder-link-path"
                onClick={() => void chooseDirectory()}
              >
                {sourcePath ? copy.folders.linkChange : copy.folders.linkChoose}
              </Button>
            </div>
            <LinkStatus check={check} />
            {check === null && (
              <p className="text-meta text-muted-foreground">
                {copy.folders.linkHint}
              </p>
            )}
          </div>
        )}

        <DialogFooter>
          <Button type="button" variant="ghost" onClick={onDone}>
            {copy.folders.cancel}
          </Button>
          <Button type="submit" disabled={!canSubmit}>
            {isRename
              ? copy.folders.save
              : sourcePath
                ? copy.folders.createLinked
                : copy.folders.create}
          </Button>
        </DialogFooter>
      </form>
    </DialogContent>
  );
}

function LinkStatus({ check }: { check: LinkCheck | null }) {
  if (check === null) return null;
  if (check.state === "refused") {
    return (
      <p role="alert" className="text-meta text-destructive">
        {check.reason}
      </p>
    );
  }
  const text =
    check.state === "checking"
      ? copy.folders.linkChecking
      : check.preview.projects === 0 && !check.preview.truncated
        ? copy.folders.linkNothingYet
        : copy.folders.linkPreview(
            check.preview.projects,
            check.preview.folders,
            check.preview.truncated,
          );
  return (
    <p role="status" className="text-meta text-muted-foreground">
      {text}
    </p>
  );
}
