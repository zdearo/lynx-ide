import { toast } from "sonner";

import { Button } from "@/components/ui/button";
import { useChangesStore } from "@/changes/changes-store";
import { CodeEditor } from "@/editor/CodeEditor";
import { FileDiffView } from "@/editor/FileDiffView";
import { FileToolbar } from "@/editor/FileToolbar";
import { documentKey, liveText } from "@/editor/editor-documents";
import { useEditorStore, type OpenFile } from "@/editor/editor-store";
import { errorMessage } from "@/lib/error-message";
import { copy } from "@/ui-copy";

function openFailure(file: OpenFile): string {
  switch (file.errorKind) {
    case "fileTooLarge":
      return copy.editor.tooLarge;
    case "binaryFile":
      return copy.editor.binary;
    case "notAFile":
      return copy.editor.notAFile;
    default:
      return file.error ?? copy.editor.openFailed;
  }
}

/** The central area while a file is on screen: a toolbar over the editor or the diff. */
export function FileEditor({
  worktreePath,
  file,
}: {
  worktreePath: string;
  file: OpenFile;
}) {
  return (
    <div className="flex h-full min-h-0 flex-col">
      <FileToolbar worktreePath={worktreePath} file={file} />
      <div className="min-h-0 flex-1">
        {file.view === "diff" ? (
          <FileDiffView
            key={`${file.path}:${file.diffSource.kind}`}
            worktreePath={worktreePath}
            file={file}
          />
        ) : (
          <EditView worktreePath={worktreePath} file={file} />
        )}
      </div>
    </div>
  );
}

function EditView({
  worktreePath,
  file,
}: {
  worktreePath: string;
  file: OpenFile;
}) {
  const store = useEditorStore.getState();
  const id = documentKey(worktreePath, file.path);

  async function save(text: string, force = false) {
    try {
      const outcome = await store.save(worktreePath, file.path, text, force);
      if (outcome === "saved") {
        void useChangesStore.getState().refresh(worktreePath);
      }
    } catch (error) {
      toast.error(copy.editor.saveFailed, { description: errorMessage(error) });
    }
  }

  if (file.status === "loading") {
    return (
      <p className="px-4 pt-3 text-ui text-muted-foreground" aria-busy="true">
        {copy.editor.loading}
      </p>
    );
  }

  if (file.status === "error") {
    return (
      <div className="px-4 pt-3" role="alert">
        <p className="text-ui text-destructive">{openFailure(file)}</p>
        {file.errorKind === null && (
          <Button
            variant="outline"
            size="sm"
            className="mt-2"
            onClick={() => void store.reload(worktreePath, file.path)}
          >
            {copy.editor.retry}
          </Button>
        )}
      </div>
    );
  }

  return (
    <div className="flex h-full min-h-0 flex-col">
      {file.conflict && (
        <div
          role="alert"
          className="flex shrink-0 flex-wrap items-center gap-x-3 gap-y-1 border-b border-border bg-sidebar px-3 py-1.5 text-ui"
        >
          <span className="font-medium">{copy.editor.conflictTitle}</span>
          <span className="text-muted-foreground">
            {copy.editor.conflictBody}
          </span>
          <span className="ml-auto flex gap-2">
            <Button
              variant="outline"
              size="xs"
              onClick={() => void store.reload(worktreePath, file.path)}
            >
              {copy.editor.reload}
            </Button>
            <Button
              size="xs"
              onClick={() => {
                const text = liveText(id);
                if (text !== undefined) void save(text, true);
              }}
            >
              {copy.editor.overwrite}
            </Button>
          </span>
        </div>
      )}
      <div className="min-h-0 flex-1">
        <CodeEditor
          key={`${id}:${file.revision}`}
          documentId={id}
          path={file.path}
          revision={file.revision}
          text={file.text}
          onChange={() => store.markDirty(worktreePath, file.path)}
          onSave={(text) => void save(text)}
        />
      </div>
    </div>
  );
}
