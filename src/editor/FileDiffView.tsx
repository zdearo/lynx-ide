import { useEffect, useState } from "react";

import { commands, type IpcErrorKind } from "@/bindings";
import { Button } from "@/components/ui/button";
import { changeOf, useChangesStore } from "@/changes/changes-store";
import { useDiffModeStore } from "@/editor/diff-mode";
import { DiffEditor } from "@/editor/DiffEditor";
import { documentKey, getDocument } from "@/editor/editor-documents";
import type { OpenFile } from "@/editor/editor-store";
import { errorMessage } from "@/lib/error-message";
import { IpcRequestError, unwrap } from "@/lib/ipc-result";
import { copy } from "@/ui-copy";

type DiffState =
  | { status: "loading" }
  | { status: "ready"; original: string; modified: string }
  | { status: "error"; kind: IpcErrorKind | null; message: string };

function failureText(state: Extract<DiffState, { status: "error" }>): string {
  switch (state.kind) {
    case "binaryFile":
      return copy.diff.binary;
    case "fileTooLarge":
      return copy.diff.tooLarge;
    default:
      return state.message;
  }
}

/** HEAD against the working tree for one file, or against the unsaved text when there is some. */
export function FileDiffView({
  worktreePath,
  file,
}: {
  worktreePath: string;
  file: OpenFile;
}) {
  const mode = useDiffModeStore((state) => state.mode);
  // A new read of the changes means the files may have changed: look again.
  const tick = useChangesStore((state) => state.byWorktree[worktreePath]?.tick);
  const modified = useChangesStore((state) =>
    Boolean(changeOf(state.byWorktree[worktreePath], file.path)),
  );
  const [state, setState] = useState<DiffState>({ status: "loading" });
  const [attempt, setAttempt] = useState(0);

  useEffect(() => {
    let current = true;
    void (async () => {
      try {
        const read = await unwrap(
          commands.readFileDiff(worktreePath, file.path, file.originalPath),
        );
        if (!current) return;
        const next = {
          status: "ready" as const,
          original: read.original ?? "",
          modified: read.modified ?? "",
        };
        // Same texts: keep the object, so the view is not rebuilt and loses its place.
        setState((previous) =>
          previous.status === "ready" &&
          previous.original === next.original &&
          previous.modified === next.modified
            ? previous
            : next,
        );
      } catch (error) {
        if (!current) return;
        setState({
          status: "error",
          kind: error instanceof IpcRequestError ? error.kind : null,
          message: errorMessage(error),
        });
      }
    })();
    return () => {
      current = false;
    };
  }, [
    worktreePath,
    file.path,
    file.originalPath,
    file.revision,
    tick,
    attempt,
  ]);

  if (state.status === "loading") {
    return (
      <p className="px-4 pt-3 text-ui text-muted-foreground" aria-busy="true">
        {copy.diff.loading}
      </p>
    );
  }

  if (state.status === "error") {
    return (
      <div className="px-4 pt-3" role="alert">
        <p className="text-ui text-destructive">{copy.diff.failed}</p>
        <p className="mt-1 text-meta break-words text-muted-foreground">
          {failureText(state)}
        </p>
        {state.kind === null && (
          <Button
            variant="outline"
            size="sm"
            className="mt-2"
            onClick={() => setAttempt((count) => count + 1)}
          >
            {copy.diff.retry}
          </Button>
        )}
      </div>
    );
  }

  // Unsaved edits belong in the diff: it answers "what will my change look like".
  const kept = file.dirty
    ? getDocument(documentKey(worktreePath, file.path))
    : undefined;
  const modifiedText =
    kept?.revision === file.revision
      ? kept.state.doc.toString()
      : state.modified;

  if (state.original === modifiedText && !modified) {
    return (
      <p className="px-4 pt-3 text-ui text-muted-foreground">
        {copy.diff.noChanges}
      </p>
    );
  }

  return (
    <DiffEditor
      path={file.path}
      original={state.original}
      modified={modifiedText}
      mode={mode}
    />
  );
}
