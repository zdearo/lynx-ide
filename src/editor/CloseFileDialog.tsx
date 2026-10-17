import {
  AlertDialog,
  AlertDialogAction,
  AlertDialogCancel,
  AlertDialogContent,
  AlertDialogDescription,
  AlertDialogFooter,
  AlertDialogHeader,
  AlertDialogTitle,
} from "@/components/ui/alert-dialog";
import { useDialogStore } from "@/dialog-store";
import { useEditorStore } from "@/editor/editor-store";
import { copy } from "@/ui-copy";

/** Asks before closing a file that has changes not saved yet. Open while a request exists. */
export function CloseFileDialog() {
  const target = useDialogStore((state) => state.closeFileTarget);
  const askCloseFile = useDialogStore((state) => state.askCloseFile);

  return (
    <AlertDialog
      open={target !== null}
      onOpenChange={(open) => {
        if (!open) askCloseFile(null);
      }}
    >
      {target && (
        <AlertDialogContent>
          <AlertDialogHeader>
            <AlertDialogTitle>
              {copy.editor.closeDialog.title(
                target.path.split("/").pop() ?? target.path,
              )}
            </AlertDialogTitle>
            <AlertDialogDescription>
              {copy.editor.closeDialog.body}
            </AlertDialogDescription>
          </AlertDialogHeader>
          <AlertDialogFooter>
            <AlertDialogCancel>
              {copy.editor.closeDialog.keep}
            </AlertDialogCancel>
            <AlertDialogAction
              variant="destructive"
              onClick={() =>
                useEditorStore
                  .getState()
                  .close(target.worktreePath, target.path)
              }
            >
              {copy.editor.closeDialog.confirm}
            </AlertDialogAction>
          </AlertDialogFooter>
        </AlertDialogContent>
      )}
    </AlertDialog>
  );
}
