import { toast } from "sonner";

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
import { errorMessage } from "@/lib/error-message";
import { copy } from "@/ui-copy";
import { isLinkedRoot } from "@/workspace/projects/linked-folders";
import { useWorkspaceStore } from "@/workspace/workspace-store";

export function DeleteFolderDialog() {
  const folder = useDialogStore((state) => state.deleteFolderTarget);
  const askDeleteFolder = useDialogStore((state) => state.askDeleteFolder);
  const deleteFolder = useWorkspaceStore((state) => state.deleteFolder);

  return (
    <AlertDialog
      open={folder !== null}
      onOpenChange={(open) => {
        if (!open) askDeleteFolder(null);
      }}
    >
      {folder && (
        <AlertDialogContent>
          <AlertDialogHeader>
            <AlertDialogTitle>
              {copy.folders.deleteTitle(folder.name)}
            </AlertDialogTitle>
            <AlertDialogDescription>
              {isLinkedRoot(folder)
                ? copy.folders.deleteLinkedBody
                : copy.folders.deleteBody}
            </AlertDialogDescription>
          </AlertDialogHeader>
          <AlertDialogFooter>
            <AlertDialogCancel>{copy.folders.cancel}</AlertDialogCancel>
            <AlertDialogAction
              variant="destructive"
              onClick={() =>
                void deleteFolder(folder.id).catch((error: unknown) => {
                  toast.error(copy.folders.failed, {
                    description: errorMessage(error),
                  });
                })
              }
            >
              {copy.folders.deleteConfirm}
            </AlertDialogAction>
          </AlertDialogFooter>
        </AlertDialogContent>
      )}
    </AlertDialog>
  );
}
