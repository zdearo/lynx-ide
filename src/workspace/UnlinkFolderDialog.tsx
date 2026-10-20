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
import { useWorkspaceStore } from "@/workspace/workspace-store";

/** Confirms that a linked folder stops following its directory. Nothing is removed. */
export function UnlinkFolderDialog() {
  const folder = useDialogStore((state) => state.unlinkFolderTarget);
  const askUnlinkFolder = useDialogStore((state) => state.askUnlinkFolder);
  const unlinkFolder = useWorkspaceStore((state) => state.unlinkFolder);

  return (
    <AlertDialog
      open={folder !== null}
      onOpenChange={(open) => {
        if (!open) askUnlinkFolder(null);
      }}
    >
      {folder && (
        <AlertDialogContent>
          <AlertDialogHeader>
            <AlertDialogTitle>
              {copy.folders.unlinkTitle(folder.name)}
            </AlertDialogTitle>
            <AlertDialogDescription>
              {copy.folders.unlinkBody}
            </AlertDialogDescription>
          </AlertDialogHeader>
          <AlertDialogFooter>
            <AlertDialogCancel>{copy.folders.cancel}</AlertDialogCancel>
            <AlertDialogAction
              onClick={() =>
                void unlinkFolder(folder.id).catch((error: unknown) => {
                  toast.error(copy.folders.unlinkFailed, {
                    description: errorMessage(error),
                  });
                })
              }
            >
              {copy.folders.unlinkConfirm}
            </AlertDialogAction>
          </AlertDialogFooter>
        </AlertDialogContent>
      )}
    </AlertDialog>
  );
}
